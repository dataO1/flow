use flow::view::app::App;
extern crate crossterm;

#[tokio::main]
async fn main() {
    let app = App::new();
    let res = app.run().await.unwrap();
    println!("App closed: {:#?}", res);
}
