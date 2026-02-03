use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::GenericArgument;
use syn::PathArguments; 
use syn::DeriveInput; 
use quote::quote;
use syn::Fields;
use syn::Data;
use syn::Type; 

#[proc_macro_derive(Entry, attributes(table_name, primary_key, skip, foreign_key, autoincrement))]
pub fn derive_entry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // get table name from attribute or use struct name in lowercase
    let table_name = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("table_name"))
        .and_then(|attr| attr.parse_args::<syn::LitStr>().ok())
        .map(|lit| lit.value())
        .unwrap_or_else(|| name.to_string().to_lowercase());

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Entry can only be derived for structs with named fields"),
        },
        _ => panic!("Entry can only be derived for structs"),
    };

    // find primary key field
    let mut primary_key_field = None;
    let mut schema_parts = Vec::new();
    let mut bind_fields = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut from_row_fields = Vec::new();
    let mut field_index = 0;

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        // skip fields with #[skip]
        if field.attrs.iter().any(|attr| attr.path().is_ident("skip")) {
            continue;
        }

        // Generate from_row conversion for each field
        let conversion = generate_row_conversion(field_index, &field.ty);
        from_row_fields.push(quote! {
            #field_name: #conversion
        });

        field_index += 1;

        // check primary key label
        let is_primary_key = field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("primary_key"));

        if is_primary_key {
            primary_key_field = Some(field_name.clone());
        }

        let foreign_key_info = field
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("foreign_key"))
            .and_then(parse_foreign_key_attr);

        let is_autoincrement = field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("autoincrement"));

        // generate schema
        let sql_type = rust_type_to_sql(&field.ty);
        let pk_constraint = if is_primary_key {
            if is_autoincrement {
                " PRIMARY KEY AUTOINCREMENT"
            } else {
                " PRIMARY KEY"
            }
        } else {
            ""
        };
        schema_parts.push(format!("{} {}{}", field_name_str, sql_type, pk_constraint));

        // Add foreign key constraint
        if let Some((ref_table, ref_column, on_delete)) = foreign_key_info {
            let on_delete_clause = on_delete
                .map(|action| format!(" ON DELETE {}", action.to_uppercase()))
                .unwrap_or_default();
            foreign_keys.push(format!(
                "FOREIGN KEY ({}) REFERENCES {}({}){}",
                field_name_str, ref_table, ref_column, on_delete_clause
            ));
        }

        // generate bind_values entries
        if !is_primary_key || !is_autoincrement {
            let conversion = generate_value_conversion(field_name, &field.ty);
            bind_fields.push(quote! {
                (#field_name_str, #conversion)
            });
        }
    }

    let primary_key_field =
        primary_key_field.expect("No primary key field found. Use #[primary_key]");

    // Combine schema parts and foreign keys
    let mut full_schema = schema_parts.join(", ");
    if !foreign_keys.is_empty() {
        full_schema.push_str(", ");
        full_schema.push_str(&foreign_keys.join(", "));
    }

    let expanded = quote! {
        impl Entryable for #name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn p_key(&self) -> usize {
                self.#primary_key_field as usize
            }

            fn schema() -> &'static str {
                #full_schema
            }

            fn bind_values(&self) -> Vec<(&'static str, rusqlite::types::Value)> {
                vec![
                    #(#bind_fields),*
                ]
            }
            fn from_row(row: &rusqlite::Row) -> Result<Self, rusqlite::Error> {
                Ok(Self {
                    #(#from_row_fields),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

fn parse_foreign_key_attr(attr: &syn::Attribute) -> Option<(String, String, Option<String>)> {
    attr.parse_args_with(|input: syn::parse::ParseStream| {
        let mut table = None;
        let mut column = None;
        let mut on_delete = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let _: syn::Token![=] = input.parse()?;
            let value: syn::LitStr = input.parse()?;

            match key.to_string().as_str() {
                "table" => table = Some(value.value()),
                "column" => column = Some(value.value()),
                "on_delete" => on_delete = Some(value.value()),
                _ => {}
            }

            if !input.is_empty() {
                let _: syn::Token![,] = input.parse()?;
            }
        }

        Ok((table, column, on_delete))
    })
    .ok()
    .and_then(|(t, c, d)| match (t, c) {
        (Some(table), Some(column)) => Some((table, column, d)),
        _ => None,
    })
}

fn generate_value_conversion(field_name: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
    if is_unsigned_int_type(ty) {
        quote! {
            rusqlite::types::Value::from(self.#field_name as i64)
        }
    } else if is_simple_type(ty) {
        quote! {
            rusqlite::types::Value::from(self.#field_name.clone())
        }
    } else if is_option_type(ty) {
        let inner_ty = extract_option_inner_type(ty);
        if let Some(inner) = inner_ty {
            if is_unsigned_int_type(inner) {
                quote! {
                    self.#field_name.as_ref().map(|v| rusqlite::types::Value::from(*v as i64))
                        .unwrap_or(rusqlite::types::Value::Null)
                }
            } else if is_simple_type(inner) {
                quote! {
                    self.#field_name.as_ref().map(|v| rusqlite::types::Value::from(v.clone()))
                        .unwrap_or(rusqlite::types::Value::Null)
                }
            } else {
                quote! {
                    self.#field_name.as_ref()
                        .map(|v| serde_json::to_string(v).unwrap())
                        .map(|s| rusqlite::types::Value::from(s))
                        .unwrap_or(rusqlite::types::Value::Null)
                }
            }
        } else {
            quote! { rusqlite::types::Value::Null }
        }
    } else {
        quote! {
            rusqlite::types::Value::from(serde_json::to_string(&self.#field_name).unwrap())
        }
    }
}

fn is_simple_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        let type_name = type_path.path.segments.last().unwrap().ident.to_string();
        matches!(
            type_name.as_str(),
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" | "f32" | "f64" | "String" | "bool"| "str"
        )
    } else {
        false
    }
}

fn is_unsigned_int_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        let type_name = type_path.path.segments.last().unwrap().ident.to_string();
        matches!(
            type_name.as_str(),
            "u8" | "u16" | "u32" | "u64" | "usize"
        )
    } else {
        false
    }
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        type_path.path.segments.last().unwrap().ident == "Option"
    } else {
        false
    }
}

fn extract_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty);
    }

    None
}

fn rust_type_to_sql(ty: &Type) -> &'static str {
    if let Type::Path(type_path) = ty {
        let type_name = type_path.path.segments.last().unwrap().ident.to_string();
        match type_name.as_str() {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" => "INTEGER",
            "f32" | "f64" => "REAL",
            "String" | "str" => "TEXT",
            "bool" => "INTEGER",
            "Option" => {
                // For Option types, check the inner type
                if let Some(inner) = extract_option_inner_type(ty) {
                    rust_type_to_sql(inner)
                } else {
                    "TEXT"
                }
            }
            "Vec" => "TEXT",
            _ => "TEXT",
        }
    } else {
        "TEXT"
    }
}

fn generate_row_conversion(index: usize, ty: &Type) -> proc_macro2::TokenStream {
    if is_option_type(ty) {
        let inner_ty = extract_option_inner_type(ty);
        if let Some(inner) = inner_ty {
            if is_simple_type(inner) {
                quote! { row.get(#index)? }
            } else {
                // For complex types stored as JSON
                quote! {
                    row.get::<_, Option<String>>(#index)?
                        .and_then(|s| serde_json::from_str(&s).ok())
                }
            }
        } else {
            quote! { None }
        }
    } else if is_simple_type(ty) {
        quote! { row.get(#index)? }
    } else {
        // For complex types stored as JSON
        quote! {
            serde_json::from_str(&row.get::<_, String>(#index)?).unwrap()
        }
    }
}
