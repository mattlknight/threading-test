use actix_web::{server, App };
// use actix_web::HttpResponse;
use actix_net::server::Server;
use futures::future::Future;
use std::io::{self, Write};
use std::error::Error;
use std::sync::mpsc;
use std::thread;
use std::time;
use actix::Addr;
use lazy_static::lazy_static;

fn main() -> Result<(),Box<Error>> {
    let srv = start_web_server();
    // thread::sleep(time::Duration::from_secs(15));
    loop_until()?;

    println!("Shutting Down Server...");
    let _ = srv.send(server::StopServer { graceful: true }).wait(); // <- Send `StopServer` message to server.

    println!("Shutdown Server");
    Ok(())
}

fn loop_until() -> Result<(),Box<Error>> {
    

    loop {
        let mut input = String::new();
        print!("Press Q then <enter> to quit server.  ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut input)?;
        input = input.trim().to_ascii_lowercase();

        if input == "q" {
            break;
        }
    }

    Ok(())
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