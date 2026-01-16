use clap::{Parser, Subcommand};
use colored::*;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio_util::codec::{Framed, LinesCodec};

#[derive(Parser)]
#[command(name = "chat-app")]
#[command(about = "Консольное приложение чата (клиент и сервер)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Server {

        #[arg(short, long, default_value_t = 8000)]
        port: u16,
    },
    Client {
        #[arg(short, long)]
        address: String,
        #[arg(short, long)]
        username: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server { port } => {
            run_server(port).await?;
        }
        Commands::Client { address, username } => {
            run_client(&address, &username).await?;
        }
    }

    Ok(())
}

async fn run_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("{}", format!("Сервер запущен на порту {}", port).green().bold());

    let (tx, _rx) = broadcast::channel::<String>(1000);
    let clients: Arc<Mutex<HashMap<usize, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut client_id_counter: usize = 0;

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("{}", format!("Новое подключение: {}", addr).cyan());

        let tx = tx.clone();
        let mut rx = tx.subscribe();
        let clients = clients.clone();
        let client_id = client_id_counter;
        client_id_counter += 1;

        tokio::spawn(async move {
            let mut framed = Framed::new(socket, LinesCodec::new());
            let username: Option<String>;

            if let Some(Ok(line)) = framed.next().await {
                if line.starts_with("USERNAME:") {
                    let user = line.strip_prefix("USERNAME:").unwrap().trim().to_string();
                    username = Some(user.clone());
                    clients.lock().await.insert(client_id, user.clone());
                    
                    let welcome_msg = format!("{} присоединился к чату", user);
                    let _ = tx.send(welcome_msg.clone());
                    println!("{}", format!("[СЕРВЕР] {}", welcome_msg).yellow());
                } else {
                    return;
                }
            } else {
                return;
            }

            let _ = framed.send("CONNECTED").await;

            let tx_for_recv = tx.clone();
            let tx_for_leave = tx.clone();
            let clients_clone = clients.clone();
            let username_clone = username.clone();

            loop {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(msg) => {
                                if let Err(_) = framed.send(msg.as_str()).await {
                                    break;
                                }
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    result = framed.next() => {
                        match result {
                            Some(Ok(line)) => {
                                if let Some(user) = &username_clone {
                                    let message = format!("{}: {}", user, line);
                                    println!("{}", format!("[СЕРВЕР] Получено: {}", message).yellow());
                                    let _ = tx_for_recv.send(message);
                                }
                            }
                            Some(Err(_)) | None => {
                                break;
                            }
                        }
                    }
                }
            }

            if let Some(user) = username {
                clients_clone.lock().await.remove(&client_id);
                let leave_msg = format!("{} покинул чат", user);
                let _ = tx_for_leave.send(leave_msg.clone());
                println!("{}", format!("[СЕРВЕР] {}", leave_msg).yellow());
            }
        });
    }
}

async fn run_client(address: &str, username: &str) -> Result<(), Box<dyn std::error::Error>> {
    let stream = TcpStream::connect(address).await?;
    println!("{}", format!("Подключено к серверу {}", address).green().bold());
    println!("{}", format!("Ваше имя: {}", username).green().bold());
    println!("{}", "Введите сообщение (или 'exit' для выхода):".cyan());

    let mut framed = Framed::new(stream, LinesCodec::new());

    framed.send(&format!("USERNAME:{}", username)).await?;

    if let Some(Ok(msg)) = framed.next().await {
        if msg == "CONNECTED" {
            println!("{}", "Успешно подключено к серверу!".green());
        }
    }

    let username_clone = username.to_string();
    let stdin = tokio::io::stdin();
    let mut stdin_reader = BufReader::new(stdin);
    let mut input_line = String::new();

    loop {
        tokio::select! {
            result = stdin_reader.read_line(&mut input_line) => {
                match result {
                    Ok(0) => {
                        break;
                    }
                    Ok(_) => {
                        let input = input_line.trim().to_string();
                        input_line.clear();
                        
                        if input == "exit" {
                            break;
                        }
                        
                        if !input.is_empty() {
                            if let Err(_) = framed.send(input.as_str()).await {
                                break;
                            }
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            result = framed.next() => {
                match result {
                    Some(Ok(line)) => {
                        display_message(&line, &username_clone);
                    }
                    Some(Err(_)) | None => {
                        println!("{}", "Соединение с сервером разорвано".red());
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n{}", "Выход из чата...".yellow());
                break;
            }
        }
    }

    Ok(())
}

fn display_message(message: &str, current_username: &str) {
    let mention_pattern = format!("@{}", current_username);
    let is_mentioned = message.contains(&mention_pattern);

    let is_own_message = message.starts_with(&format!("{}:", current_username));

    if is_mentioned {
        println!("{}", message.bright_yellow().on_black().bold());
    } else if is_own_message {
        println!("{}", message.cyan());
    } else {
        println!("{}", message.white());
    }
}
