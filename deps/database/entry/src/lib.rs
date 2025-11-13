use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::GenericArgument;
use syn::PathArguments; 
use syn::DeriveInput; 
use quote::quote;
use syn::Fields;
use syn::Data;
use syn::Type; 
use syn::Meta;

#[proc_macro_derive(Entry, attributes(table_name, primary_key, skip, foreign_key))]
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

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        // skip fields with #[skip]
        if field.attrs.iter().any(|attr| attr.path().is_ident("skip")) {
            continue;
        }

        // check primary key label
        let is_primary_key = field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("primary_key"));

        if is_primary_key {
            primary_key_field = Some(field_name.clone());
        }

        // check for foreign key: #[foreign_key(table = "anime", column = "id")]
        let foreign_key_info = field
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("foreign_key"))
            .and_then(parse_foreign_key_attr);

        // generate schema
        let sql_type = rust_type_to_sql(&field.ty);
        let pk_constraint = if is_primary_key { " PRIMARY KEY" } else { "" };
        schema_parts.push(format!("{} {}{}", field_name_str, sql_type, pk_constraint));

        // Add foreign key constraint
        if let Some((ref_table, ref_column)) = foreign_key_info {
            foreign_keys.push(format!(
                "FOREIGN KEY ({}) REFERENCES {}({})",
                field_name_str, ref_table, ref_column
            ));
        }

        // generate bind_values entries
        if !is_primary_key {
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
        }
    };

    TokenStream::from(expanded)
}

// Parse the foreign_key attribute: #[foreign_key(table = "anime", column = "id")]
fn parse_foreign_key_attr(attr: &syn::Attribute) -> Option<(String, String)> {

    attr.parse_args::<syn::Meta>().ok().and_then(|meta| {
        if let Meta::List(list) = meta {
            let mut table = None;
            let mut column = None;

            for token in list.tokens.clone().into_iter() {
                // This is a simplified parser - you might need more robust parsing
                let token_str = token.to_string();
                if token_str.contains("table") {
                    // Extract the table name
                    if let Some(start) = token_str.find('"')
                        && let Some(end) = token_str.rfind('"')
                    {
                        table = Some(token_str[start + 1..end].to_string());
                    }
                } else if token_str.contains("column") {
                    // Extract the column name
                    if let Some(start) = token_str.find('"')
                        && let Some(end) = token_str.rfind('"')
                    {
                        column = Some(token_str[start + 1..end].to_string());
                    }
                }
            }

            match (table, column) {
                (Some(t), Some(c)) => Some((t, c)),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn generate_value_conversion(field_name: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
    if is_simple_type(ty) {
        quote! {
            rusqlite::types::Value::from(self.#field_name.clone())
        }
    } else if is_option_type(ty) {
        let inner_ty = extract_option_inner_type(ty);
        if let Some(inner) = inner_ty {
            if is_simple_inner_type(inner) {
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
            "i32" | "i64" | "f32" | "f64" | "String" | "bool"
        )
    } else {
        false
    }
}

fn is_simple_inner_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        let type_name = type_path.path.segments.last().unwrap().ident.to_string();
        matches!(
            type_name.as_str(),
            "i32" | "i64" | "f32" | "f64" | "String" | "bool" | "str"
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
            "i32" | "i64" | "u32" | "u64" | "isize" | "usize" => "INTEGER",
            "f32" | "f64" => "REAL",
            "String" | "str" => "TEXT",
            "bool" => "INTEGER",
            "Vec" | "Option" => "TEXT",
            _ => "TEXT",
        }
    } else {
        "TEXT"
    }
}
