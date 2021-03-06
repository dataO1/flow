use flow::view::app::App;
extern crate crossterm;

#[tokio::main]
async fn main() {
    // let tempo = Analyzer::get_tempo("music/bass_symptom.mp3");
    // println!("{}", tempo);
    let app = App::default();
    let res = app.run().await.unwrap();
    println!("App closed: {:#?}", res);
}
