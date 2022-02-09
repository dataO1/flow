use crossterm::{
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::enable_raw_mode,
};
use flow::core::player::{Command, Message, Player};

use tokio::sync::mpsc::Sender;

#[tokio::main]
async fn main() {
    let player_channel = Player::spawn().await;
    player_channel
        .send(Message::Command(Command::Load(String::from(
            "music/bass_symptom.mp3",
        ))))
        .await
        .unwrap();
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
