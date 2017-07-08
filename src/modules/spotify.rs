use std::collections::HashMap;
use hyper;
use hyper::Client;
use hyper::net::{OpensslClient, HttpsConnector};
use hyper::header::{Authorization, Bearer};
use hyper::status::StatusCode;
use module;
use std::io::Read;
use discord::model::Message;
use bot::Bot;
use serde_json;
use serde_json::Value;

pub struct Module<'a> {
    commands: HashMap<u32, &'a [&'a str]>,
    pub api_key: String,
}

enum Commands {
    Search = 0,
}

impl<'a> module::Module for Module<'a> {
    fn new() -> Self {
        let mut map: HashMap<u32, &[&str]> = HashMap::new();
        static SEARCH: [&'static str; 2] = ["spotify", "sp"];
        map.insert(Commands::Search as u32, &SEARCH);
        Module {
            commands: map,
            api_key: String::new(),
        }
    }

    fn name(&self) -> &'static str {
        "Spotify"
    }

    fn description(&self) -> &'static str {
        "Commands for searching Spotify"
    }

    fn commands(&self) -> &HashMap<u32, &[&str]> {
        &self.commands
    }

    fn command_description(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Search as u32 => {
                "`!spotify, !sp {type} search_term`: Searches Spotify"
            }
            _ => "invalid id",
        }
    }

    fn command_help_message(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Search as u32 => {
                "`!sp ?(track|artist|album|playlist) search_term`: Searches Spotify for the given item and embeds it in chat"
            }
            _ => "invalid id",
        }
    }

    fn handle(&self, bot: &Bot, message: &Message, id: u32, text: &str) {
        match id {
            x if x == Commands::Search as u32 => {
                let mut args = String::new();
                let mut url = String::from("https://api.spotify.com/v1/search?type=");
                let req_type = match text.split_whitespace().nth(0) {
                    Some(v) => v,
                    None => {
                        bot.send(message.channel_id, "invalid invocation of !spotify");
                        return;
                    }
                };
                match req_type {
                    "album" | "track" => {
                        args = String::from(text.split_at(6).1);
                        url.push_str(req_type);
                    }
                    "artist" => {
                        args = String::from(text.split_at(7).1);
                        url.push_str(req_type);
                    }
                    "playlist" => {
                        args = String::from(text.split_at(9).1);
                        url.push_str(req_type);
                    }
                    _ => {
                        bot.send(message.channel_id, "invalid invocation of !spotify");
                        return;
                    }
                }
                url.push_str("&q=");
                url.push_str(&args);
                let parsed_url = hyper::Url::parse(url.as_str()).unwrap();
                let tls = OpensslClient::default();
                let connector = HttpsConnector::new(tls);
                let client = Client::with_connector(connector);
                println!("{}", self.api_key.as_str());
                let header = Authorization(Bearer { token: self.api_key.clone() });
                println!("header: {:?}", header);
                let mut response = client.get(parsed_url).header(header).send().unwrap();
                let status = response.status;
                let mut json = String::new();
                if let Err(e) = response.read_to_string(&mut json) {
                    println!("oh crap {}", e);
                }
                let json_root = serde_json::from_str::<Value>(json.as_str()).unwrap();
                if status == StatusCode::Ok {
                    let pointer = format!("/{}s/items/0/external_urls/spotify", req_type);
                    let item_url = json_root.pointer(&pointer).unwrap().as_str().unwrap();
                    bot.send(message.channel_id, item_url);
                } else {
                    bot.send(message.channel_id,
                             &format!("Spotify doesn't want you to do that: {:?}\n{}", status, json_root.pointer("/error/message").unwrap().as_str().unwrap()));
                }
            }
            _ => {
                bot.send(message.channel_id, "Invalid ID");
            }
        }
    }
}
