use actix_web::{server, App };
// use actix_web::HttpResponse;
use actix_net::server::Server;
use futures::future::Future;
use std::io::{self, Write};
use std::error::Error;
use std::sync::mpsc::{self, RecvError, TryRecvError, Receiver, SyncSender, SendError};
use std::thread::{self, JoinHandle};
use std::time;
use actix::Addr;
use lazy_static::lazy_static;

pub const MSG_POLL_DELAY: time::Duration = time::Duration::from_millis(100);
pub const MSG_POLL_DELAY_LONG: time::Duration = time::Duration::from_millis(1000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadMsg {
    Continue,
    Pause,
    Sleep(time::Duration),
    Maintenance,
    Shutdown,
    Acknowledge,
}

// pub enum ThreadState {
//     Starting,
//     Running,
//     Pausing,
//     Paused,
//     Sleeping,
//     Waking,
//     ShuttingDown,
//     ShutDown,
// }

pub struct ThreadManager {
    to_thread: SyncSender<ThreadMsg>,
    from_thread: Receiver<ThreadMsg>,
    thread_handle: JoinHandle<()>,
}

impl ThreadManager {
    pub fn new_loop<F>(func: F) -> Self
        where F:'static + Fn(SyncSender<ThreadMsg>) + Send {

        let (to_parent, from_parent) = mpsc::sync_channel(2);
        let (to_thread, from_thread) = mpsc::sync_channel(2);
        let handle = thread::spawn(move || {
            loop {
                println!("new_loop.loop() TOP");
                func(to_parent.clone());

                thread::sleep(MSG_POLL_DELAY);
                let msg = match from_parent.try_recv() {
                    Ok(msg) => msg,
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => panic!("Thread's recv channel from parent disconnected"),
                };

                to_parent.send(ThreadMsg::Acknowledge).expect("Failed to send parent message");
                match msg {
                    ThreadMsg::Shutdown => {
                        println!("new_loop received shutdown message");
                        println!("Breaking new_loop.loop()");
                        break;
                    },
                    _ => unimplemented!(),
                }

                // println!("new_loop.loop() BOTTOM"); << Unreachable due to msg match
            }
            println!("Reached end of new_loop thread");  
        });

        Self {
            to_thread,
            from_thread,
            thread_handle: handle,
        }
    }

    pub fn wait_for<F>(expected_msg: ThreadMsg, from_parent: Receiver<ThreadMsg>, func: F) -> Self
        where F:'static + Fn() + Send {

        // let (to_parent, from_parent) = mpsc::sync_channel(2);
        let (to_thread, from_thread) = mpsc::sync_channel(2);
        let handle = thread::spawn(move || {
            loop {
                println!("wait_for.loop() TOP");
                
                thread::sleep(MSG_POLL_DELAY_LONG);
                let msg = match from_parent.try_recv() {
                    Ok(msg) => msg,
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => panic!("Thread's recv channel from parent disconnected"),
                };
                if msg == expected_msg {
                    println!("Got expected message!");
                    func();
                    println!("Breaking wait_for.loop()");
                    break;
                } else if msg == ThreadMsg::Shutdown {
                    println!("Got shutdown message!");
                    println!("Breaking wait_for.loop()");
                    break;
                }
                println!("wait_for.loop() BOTTOM");            
            }
            println!("Reached end of wait_for thread");     
        });

        Self {
            to_thread,
            from_thread,
            thread_handle: handle,
        }
    }

    pub fn send(&self, msg: ThreadMsg) -> Result<(),SendError<ThreadMsg>> {
        Ok(self.to_thread.send(msg)?)
    }

    pub fn recv(&self) -> Result<ThreadMsg,RecvError> {
        Ok(self.from_thread.recv()?)
    }

    pub fn try_recv(&self) -> Result<ThreadMsg,TryRecvError> {
        Ok(self.from_thread.try_recv()?)
    }

    pub fn join(self) {
        match self.thread_handle.join() {
            Ok(_) => {},
            Err(_) => println!("Thread panicked before joining"),
        }
    }
}

fn main() -> Result<(),Box<Error>> {
    let srv = start_web_server();
    let (tx, rx) = mpsc::channel();

    let thread_1 = ThreadManager::new_loop(move |to_parent| {
        let response = ask_question("Press Q then <enter> to quit server."); // Blocking
        if response == "q" {
            to_parent.send(ThreadMsg::Shutdown).expect("Parent receiver channel disconnected");
            tx.send(ThreadMsg::Shutdown).expect("Parent receiver channel disconnected");
        }
    });

    let thread_2 = ThreadManager::wait_for(ThreadMsg::Shutdown, rx, move || {
        println!("Shutting Down Server...");
        let _ = srv.send(server::StopServer { graceful: true }).wait(); // <- Send `StopServer` message to server.
    });

    println!("Joining thread 1");
    thread_1.join(); // Blocking
    println!("Joined thread 1");
    thread_2.join(); // Blocking
    println!("Joined thread 2");

    println!("Shutdown Server");
    Ok(())
}

fn ask_question(question: &str) -> String {
        let mut input = String::new();
        println!("{}", question);
        
        io::stdout().flush().expect("Failed flushing stdout");
        io::stdin().read_line(&mut input).expect("Failed to read line from stdin");
        input.trim().to_ascii_lowercase()
}

fn start_web_server() -> Addr<Server> {
    let (tx, rx) = mpsc::channel();
    lazy_static!{
        pub static ref START_TIME: time::Instant = time::Instant::now();
    }

    thread::spawn(move || {
        println!("Started server, uptime is {}ms", START_TIME.elapsed().subsec_millis());
        
        let sys = actix::System::new("http-server");
        let addr = server::new(|| {
            App::new()
                .resource("/", |r| r.f(|_| {
                    println!("\nGot a request, Uptime: {} seconds", START_TIME.elapsed().as_secs());
                    format!(" Uptime: {} seconds", START_TIME.elapsed().as_secs())
                }))
        })
            .bind("127.0.0.1:8888").expect("Can not bind to 127.0.0.1:0")
            .shutdown_timeout(60)    // <- Set shutdown timeout to 60 seconds
            .start();
        let _ = tx.send(addr);
        let _ = sys.run();
    });

    let addr = rx.recv().expect("Failed to receive actix-web server addr");
    addr
}