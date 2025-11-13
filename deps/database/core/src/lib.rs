use rusqlite::Connection;
use rusqlite::Error;

pub trait Entryable {
    fn table_name() -> &'static str;
    fn p_key(&self) -> usize;
    fn schema() -> &'static str;
    fn bind_values(&self) -> Vec<(&'static str, rusqlite::types::Value)>;
}

pub struct DatabaseManager {
    connection: Connection
}

impl DatabaseManager {
    pub fn new<T: Into<String>>(db_path: T) -> Result<Self, Error> {
        let connection = Connection::open(db_path.into())?;
        connection.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(Self { connection })
    }

    // create table
    pub fn create_table<T: Entryable>(&self) -> Result<(), Error> {
        let table_name = T::table_name();
        let schema = T::schema();
        let query = format!("CREATE TABLE IF NOT EXISTS {} ({})", table_name, schema);
        self.connection.execute(&query, [])?;
        Ok(())
    }

    // insert
    pub fn insert<T: Entryable>(&self, obj: T) -> Result<(), Error> {
        let table_name = T::table_name();
        let bindings = obj.bind_values();
        let (names, values): (Vec<_>, Vec<_>) = bindings.into_iter().unzip();
        let placeholders: Vec<String> = (1..=values.len())
            .map(|i| format!("?{}", i))
            .collect();
        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            names.join(", "),
            placeholders.join(", ")
        );
        self.connection.execute(&query, rusqlite::params_from_iter(values))?;
        Ok(())
    }

    // update
    pub fn update<T: Entryable>(&self, obj: T, condition: &str) -> Result<(), Error> {
        let table_name = T::table_name();
        let bindings = obj.bind_values();
        let set_clauses: Vec<String> = bindings.iter()
            .map(|(name, _)| format!("{} = ?", name))
            .collect();
        let values: Vec<rusqlite::types::Value> = bindings.into_iter()
            .map(|(_, value)| value)
            .collect();
        let query = format!(
            "UPDATE {} SET {} WHERE {}",
            table_name,
            set_clauses.join(", "),
            condition
        );
        self.connection.execute(&query, rusqlite::params_from_iter(values))?;
        Ok(())
    }

    // delete
    pub fn delete<T: Entryable>(&self, obj: T) -> Result<(), Error> {
        let table_name = T::table_name();
        let condition = format!("id = {}", obj.p_key());
        let query = format!(
            "DELETE FROM {} WHERE {}",
            table_name,
            condition
        );
        self.connection.execute(&query, [])?;
        Ok(())
    }
}
