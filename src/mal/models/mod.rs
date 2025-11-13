pub mod anime;
pub mod user;

use serde::{Serialize, Deserialize};
use database::{Entry, Entryable};


pub fn na() -> String{
    "N/A".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Entry)]
#[table_name = "watch_history"]
pub struct WatchHistory {
    #[primary_key]
    pub id: usize,

    #[foreign_key(table = "anime", column = "id")]
    pub anime_id: usize,

    pub timestamp: String,
    pub episode: i32,
    pub current_time: String,
    pub total_time: String,
    pub percentage: u8,
    pub is_completed: bool,
}
