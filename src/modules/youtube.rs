use std::collections::HashMap;
use hyper;
use hyper::Client;
use hyper::net::{OpensslClient, HttpsConnector};
use module;
use std::io::Read;
use discord::model::Message;
use bot::Bot;
use hyper::status::StatusCode;
use serde_json;
use serde_json::Value;

pub struct Module<'a, 'b> {
    commands: HashMap<u32, &'a [&'a str]>,
    api_key: &'b str,
}

enum Commands {
    Embed = 0,
}

impl<'a, 'b> module::Module for Module<'a, 'b> {
    fn new() -> Result<Box<module::Module>, String> {
        let mut map: HashMap<u32, &[&str]> = HashMap::new();
        static EMBED: [&'static str; 2] = ["youtube", "yt"];
        map.insert(Commands::Embed as u32, &EMBED);
        let key = {
            let temp;
            if let Some(v) = ::CONF.pointer("/google_key") {
                if let Some(key) = v.as_str() {
                    temp = key
                } else {
                    return Err("failed to get google key".into());
                }
            } else {
                return Err("failed to get google key".into());
            }
            temp
        };
        Ok(Box::new(Module {
                        commands: map,
                        api_key: key,
                    }))
    }

    fn name(&self) -> &'static str {
        "YouTube"
    }

    fn description(&self) -> &'static str {
        "Commands for embedding YouTube videos"
    }

    fn commands(&self) -> &HashMap<u32, &[&str]> {
        &self.commands
    }

    fn command_description(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Embed as u32 => "`!youtube, !yt search term`: Searches YouTube",
            _ => "invalid id",
        }
    }

    fn command_help_message(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Embed as u32 => "`!yt search term`: Searches YouTube for the given video and embeds it in chat",
            _ => "invalid id",
        }
    }

    fn handle(&self, bot: &Bot, message: &Message, id: u32, text: &str) {
        match id {
            x if x == Commands::Embed as u32 => {
                let mut url = String::from("https://www.googleapis.com/youtube/v3/search?part=snippet&key=");
                url.push_str(&self.api_key);
                url.push_str("&q=");
                url.push_str(text);
                let parsed_url = hyper::Url::parse(url.as_str()).unwrap();
                let tls = OpensslClient::default();
                let connector = HttpsConnector::new(tls);
                let client = Client::with_connector(connector);
                let mut response = client.get(parsed_url).send().unwrap();
                let status = response.status;
                if status == StatusCode::Ok {
                    let mut json = String::new();
                    if let Err(e) = response.read_to_string(&mut json) {
                        println!("oh crap {}", e);
                    }
                    let json_root = serde_json::from_str::<Value>(json.as_str()).unwrap();
                    let video_id_ = json_root.pointer("/items/0/id/videoId").unwrap();
                    let video_id = video_id_.as_str().unwrap();
                    bot.send(message.channel_id,
                             format!("https://youtu.be/{}", video_id).as_str());
                } else {
                    bot.send(message.channel_id, "Google doesn't want you to do that");
                }
            }
            _ => {
                bot.send(message.channel_id, "Invalid ID");
            }
        }
    }
}
