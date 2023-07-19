use async_std::net::{TcpListener, TcpStream, SocketAddr};
use async_std::prelude::*;
use async_std::task::{self, TaskId};
use futures::join;
use async_std::channel::{Sender, Receiver, bounded};
use std::sync::{Arc,Mutex};


type SENDER=Sender<Message>;
type RECEIVER=Receiver<Message>;
type AuthorId=TaskId;


struct Message{
    content: [u8; 128],
    author_id: task::TaskId,
}

struct Broadcast{
    connections: Vec<(TcpStream, AuthorId)>
}

impl Broadcast{
    async fn send_except(&mut self, msg: Message){
        for (strm, author_id) in &mut self.connections{
            if *author_id != msg.author_id {
                let bytes = strm.write(&msg.content).await.expect("Failed to broadcast");
                println!("{} Bytes sent from {} to ALL", bytes, msg.author_id);
            }
        }
    }

    fn new() -> Self {
        Broadcast { connections: Vec::new() }
    }

    fn subscribe(&mut self, stream: TcpStream, author_id: AuthorId){
        self.connections.push((stream, author_id));
        
    }
}

async fn handle_client(conn: (TcpStream, SocketAddr), tx: SENDER){
    let (mut st, addr) = conn;
    println!("Connection from {}", addr.to_string());
    let mut count = 0;
    loop {
        let mut msg = [0u8; 128];
        let res = st.read(&mut msg).await.unwrap_or(0);
        if res == 0{
            count += 1;
            if count == 4 {
                println!("shutting down client {}", addr.to_string());
                st.shutdown(std::net::Shutdown::Both).expect("Shutdown failed");
                break;
            }
            continue;
        }
        println!("Bytes read: {res}");
        let message = Message{content: msg, author_id: task::current().id()};
        tx.send(message).await.expect("Error sending message!");
        
    }
    println!("Stopped Handling! [Task id: {}]", task::current().id());
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let (tx, rx): (SENDER, RECEIVER) = bounded(10);
    let broadcast = Arc::new(Mutex::new(Broadcast::new()));

    
    let listner = TcpListener::bind("127.0.0.1:2449").await?;

    let t1 = async {
        let broadcast= broadcast.clone();
        loop {
            let msg = rx.recv().await.expect("Error receiving message");
            println!("[Author {}]: {}", msg.author_id, String::from_utf8_lossy(&msg.content).to_string());
            let mut g = broadcast.lock().unwrap();
            g.send_except(msg).await;
            drop(g);
        }
    };

    let t2 = async{
        let broadcast = broadcast.clone();
        loop {
            let conn = listner.accept().await.expect("something went wrong");
            let tsk = task::spawn(handle_client(conn.clone(), tx.clone()));
            let mut g = broadcast.lock().unwrap();
            g.subscribe(conn.0, tsk.task().id());
            drop(g);
        }
    };

    join!(t1, t2);
    Ok(())
}