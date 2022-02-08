use flow::core::player::{Command, Message, Player};
extern crate crossterm;

use crossterm::{
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::enable_raw_mode,
};

use tokio::sync::mpsc::Sender;

#[tokio::main]
async fn main() {
    let player_channel = Player::init("music/bass_symptom.mp3").await;
    // tokio::spawn(async move {
    //     println!("listener process spawned");
    //     match rx.recv().await {
    //         Some(Message::Command(Command::PlayerStart)) => {
    //             println!("player start command received");
    //         }
    //         Some(_) => {
    //             println!("received unexpected response");
    //             Player::init(&mut player.reader).await
    //         }
    //         None => {
    //             println!("channel closed or no remaining messages in buffer");
    //             Player::init(&mut player.reader).await
    //         }
    //     }
    // });
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
                println!("Sending PlayerStart Command");
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
                break;
            }
            _ => print!(""),
        }
    }
}
