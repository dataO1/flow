use flow::core::player::{Command, Message, Player};
#[macro_use]
extern crate crossterm;

use crossterm::cursor;
use crossterm::event::{read, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use std::io::{stdout, Write};
use tokio::sync::mpsc::Sender;

#[tokio::main]
async fn main() {
    let mut player = Player::new("music/bass_symptom.mp3");
    let mut rx = player.rx;
    tokio::spawn(async move {
        println!("listener process spawned");
        match rx.recv().await {
            Some(Message::Command(Command::PlayerStart)) => {
                println!("player start command received");
                Player::toggle_play(&mut player.reader)
            }
            Some(Message::Command(Command::PlayerStop)) => {
                println!("player stop command received");
            }
            Some(Message::Command(Command::Unknown)) => {
                println!("unknown command received");
            }
            Some(Message::Response(_res)) => {
                println!("received unexpected response");
            }
            None => (),
        }
    });
    input_loop(player.tx).await;
    ()
}

async fn input_loop(player: Sender<Message>) {
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
                let res = player
                    .send(Message::Command(Command::PlayerStart))
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
