pub mod watchlist;
pub mod yellowstone;

use watchlist::WatchList;

#[tokio::main]
async fn main() {
    let _watchlist = WatchList::new();
    println!("Indexer started");
}
