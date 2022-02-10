use flow::core::player::{Command, Message, Player};
use flow::view::app;
extern crate crossterm;

use crossterm::{
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::enable_raw_mode,
};

use tokio::sync::mpsc::Sender;

#[tokio::main]
async fn main() {
    let app = app::App::new().unwrap();
    app.run();
    let player_channel = Player::spawn().await;
    player_channel
        .send(Message::Command(Command::Load(String::from(
            "music/bass_symptom.mp3",
        ))))
        .await
        .unwrap();
    // start input loop
    input(player_channel).await;
    ()
}

async fn input(player: Sender<Message>) {
    // let stdout = stdout();
    //going into raw mode
    enable_raw_mode().unwrap();

    //key detection
    loop {
        match read().unwrap() {
            Event::Key(KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
            }) => {
                player
                    .send(Message::Command(Command::TogglePlay))
                    .await
                    .unwrap();
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                println!("exit");
                player.send(Message::Command(Command::Close)).await.unwrap();
                break;
            }
            _ => print!(""),
        }
    }
}
