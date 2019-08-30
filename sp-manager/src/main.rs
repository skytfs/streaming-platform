use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::net::SocketAddr;
use serde_derive::Deserialize;
use sysinfo::{ProcessExt, SystemExt};
use warp::Filter;

#[derive(Debug, Deserialize)]
struct Hub {
    file_name: Option<String>,
    config: Option<toml::Value>
}

#[derive(Debug, Deserialize)]
struct Service {
    file_name: Option<String>,
    config: Option<toml::Value>
}

struct Process {
    pub id: usize,
    pub name: String
}

struct StartedProcess {    
    pub name: String,
    pub r#type: ProcessType,
    pub instance: std::process::Child
}

enum ProcessType {
    Hub,
    Service
}

enum Msg {
    StartProcess(String),
    StopProcess(String)
}

/// This is what we're going to decode into. Each field is optional, meaning
/// that it doesn't have to be present in TOML.
#[derive(Debug, Deserialize)]
struct Config {
    hub_path: Option<String>,
    service_path: Option<String>,
    hubs: Option<Vec<Hub>>,
    services: Option<Vec<Service>>
}

fn main() {
    let config_path = std::env::args().nth(1)
        .expect("config location not passed as argument");

    let file = File::open(config_path)
        .expect("failed to open config");

    let mut buf_reader = BufReader::new(file);

    let mut config = String::new();

    buf_reader.read_to_string(&mut config)
        .expect("failed to read config");

    let config: Config = toml::from_str(&config).unwrap();
    println!("{:#?}", config);

    println!("quering system data");

    let mut system = sysinfo::System::new();

    // First we update all information of our system struct.
    system.refresh_all();

    let mut running = vec![];

    for (pid, proc_) in system.get_process_list() {
        //println!("{}:{} => status: {:?}", pid, proc_.name(), proc_.status());

        running.push(Process {
            id: *pid as usize,
            name: proc_.name().to_owned()
        });
    }

    fix_running_self(&running);    

    let mut started = std::collections::HashMap::new();    

    match config.hubs {
        Some(hubs) => {
            let hub_path = config.hub_path.clone()
                .expect("hub path is empty, but hubs are present");

            for hub in hubs {

                match hub.file_name {
                    Some(file_name) => {
                        
                        fix_running(&running, &file_name);

                        println!("starting {}", file_name);

                        let instance = match hub.config {
                            Some(config) => {
                                std::process::Command::new(hub_path.clone() + "/" + &file_name)
                                    .arg(toml::to_string(&config)
                                        .expect("serialization to TOML string failed, check hub config")
                                    )
                                    .spawn()
                                    .expect(&format!("{} command failed to start", file_name))
                            }
                            None => {
                                std::process::Command::new(hub_path.clone() + "/" + &file_name)
                                    .spawn()
                                    .expect(&format!("{} command failed to start", file_name))
                            }
                        };

                        started.insert(file_name.clone(), StartedProcess {
                            name: file_name.clone(),
                            r#type: ProcessType::Hub,
                            instance
                        });

                        println!("done starting {}", file_name);
                    }
                    None => {
                        println!("hub with empty file name, please note");
                    }
                }

            }
        }
        None => {
            println!("no hubs are configured to run");
        }
    }

    match config.services {
        Some(services) => {
            let service_path = config.service_path.clone()
                .expect("service path is empty, but services are present");

            for service in services {
                match service.file_name {
                    Some(file_name) => {

                        fix_running(&running, &file_name);

                        println!("starting {}", file_name);

                        let instance = match service.config {
                            Some(config) => {
                                std::process::Command::new(service_path.clone() + "/" + &file_name)
                                    .arg(toml::to_string(&config)
                                        .expect("serialization to TOML string failed, check service config")
                                    )
                                    .spawn()
                                    .expect(&format!("{} command failed to start", file_name))
                            }
                            None => {
                                std::process::Command::new(service_path.clone() + "/" + &file_name)                                    
                                    .spawn()
                                    .expect(&format!("{} command failed to start", file_name))
                            }
                        };

                        started.insert(file_name.clone(), StartedProcess {
                            name: file_name.clone(),
                            r#type: ProcessType::Service,
                            instance
                        });

                        println!("done starting {}", file_name);
                    }
                    None => {
                        println!("service with empty file name, please note");
                    }
                }
            }

        }
        None => {
            println!("no services are configured to run");
        }
    }

    println!("starting command server");

    let routes = warp::path("hello")
        .and(warp::header("user-agent"))
        .map(|agent: String| {
            format!("Hello, your agent is {}", agent)
        })
        .or(
            warp::path("stop")
            .and(warp::path::param())
            .map(|name: String| {
                //let q = started.get(&name);

                "".to_owned()
            })
        )
        .or(
            warp::path("start")
            .and(warp::path::param())
            .map(|name: String| {
                "".to_owned()
            })
        )
        ;
	
    let addr = "0.0.0.0:49999".parse::<SocketAddr>().unwrap();    
    warp::serve(routes).run(addr);
}

fn fix_running_self(running: &Vec<Process>) {
    let name = std::env::current_exe()
        .expect("failed to get current_exe result")
        .file_name()
        .expect("empty file name for current_exe")
        .to_str()
        .expect("failed to convert file name OsStr to str")
        .to_owned();

    println!("fixing running processes for {}", name);

    let id = std::process::id() as usize;

    for process in running {
        if process.name == name && process.id != id {
            stop_process(process.id, &process.name);
        }
    }

    println!("done for {}", name);
}

fn fix_running(running: &Vec<Process>, name: &str) {
    println!("fixing running processes for {}", name);

    for process in running {
        if process.name == name {
            stop_process(process.id, &process.name);
        }
    }

    println!("done for {}", name);
}

fn stop_process(id: usize, name: &str) {
    println!("attempt to stop process {} with id {}", name, id);

    if cfg!(windows) {
        std::process::Command::new("taskkill")
            .arg("/F")
            .arg("/PID")
            .arg(id.to_string())
            .output()
            .expect(&format!("process stop failed, {}, id {}", name, id));
    }
    else {
        std::process::Command::new("kill")            
            .arg(id.to_string())
            .output()
            .expect(&format!("process stop failed, {}, id {}", name, id));
    }

    println!("process stop ok, {}, id {}", name, id);
}
