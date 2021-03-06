use bot::Bot;
use discord::*;
use discord::model::*;
use module;
use regex::Regex;
use serde_json;
use std;
use std::collections::BTreeMap;
use std::collections::hash_map::HashMap;
use std::error;
use std::fmt;
use std::fs::File;
use std::io;
use std::sync::{RwLock, RwLockReadGuard};

#[derive(Serialize, Deserialize)]
struct Memory {
    // The map is from ServerId into an array of RoleIds.
    admin_roles: BTreeMap<String, Vec<u64>>,
}

pub struct Module<'a> {
    commands: HashMap<u32, &'a [&'a str]>,
    memory: RwLock<Memory>,
}

lazy_static! {
	static ref NUKE_REGEX: Regex = Regex::new(r"\s*(([0-9]+)(\s|$)).*").unwrap();
	static ref ADMIN_REGEX: Regex = Regex::new(r"\s*(list|add|remove)(\s|$).*").unwrap();
}
const MEMORY_FILENAME: &'static str = "memory.json";

enum Commands {
    Admin = 0,
    Nuke = 1,
}

impl Memory {
    fn load_from_file() -> MyResult<Self> {
        let file = try!(File::open(MEMORY_FILENAME));
        let mut memory: Memory = try!(serde_json::de::from_reader(file));

        let mut keys_to_remove = Vec::new();
        for (server, roles) in &mut memory.admin_roles {
            if roles.len() == 0 {
                keys_to_remove.push(server.clone());
            } else {
                roles.sort();
                roles.dedup();
            }
        }

        for key in keys_to_remove {
            memory.admin_roles.remove(&key);
        }

        Ok(memory)
    }

    fn save_to_file(&self) -> MyResult<()> {
        let mut file = try!(File::create(MEMORY_FILENAME));
        try!(serde_json::ser::to_writer(&mut file, &self));

        Ok(())
    }

    pub fn get_admin_roles(&self, server: ServerId) -> Option<&Vec<u64>> {
        self.admin_roles.get(&server.0.to_string())
    }

    pub fn remove_admin_roles(&mut self, server: ServerId, roles: &Vec<RoleId>) {
        let mut remove = false;

        if let Some(server_admin_roles) = self.admin_roles.get_mut(&server.0.to_string()) {
            server_admin_roles.retain(|x| roles.iter().filter(|r| r.0 == *x).next().is_none());

            if server_admin_roles.len() == 0 {
                remove = true;
            }
        }

        if remove {
            self.admin_roles.remove(&server.0.to_string());
        }

        if let Err(err) = self.save_to_file() {
            println!("[CRITICAL] Could not save memory to file: {}", err);
        }
    }

    pub fn add_admin_roles(&mut self, server: ServerId, roles: &Vec<RoleId>) {
        {
            let server_admin_roles = self.admin_roles
                .entry(server.0.to_string())
                .or_insert(Vec::new());

            for role in roles {
                server_admin_roles.push(role.0);
            }

            server_admin_roles.sort();
            server_admin_roles.dedup();
        }

        if let Err(err) = self.save_to_file() {
            println!("[CRITICAL] Could not save memory to file: {}", err);
        }
    }
}

#[derive(Debug)]
enum MyError {
    IO(io::Error),
    Json(serde_json::error::Error),
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            MyError::IO(ref err) => write!(f, "IO error: {}", err),
            MyError::Json(ref err) => write!(f, "JSON error: {}", err),
        }
    }
}

impl error::Error for MyError {
    fn description(&self) -> &str {
        match *self {
            MyError::IO(ref err) => err.description(),
            MyError::Json(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            MyError::IO(ref err) => Some(err),
            MyError::Json(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for MyError {
    fn from(err: io::Error) -> MyError {
        MyError::IO(err)
    }
}

impl From<serde_json::error::Error> for MyError {
    fn from(err: serde_json::error::Error) -> MyError {
        MyError::Json(err)
    }
}

type MyResult<T> = std::result::Result<T, MyError>;

impl<'a> module::Module for Module<'a> {
    fn new() -> std::result::Result<Box<module::Module>, String> {
        let mut map: HashMap<u32, &[&str]> = HashMap::new();
        static ADMIN: [&'static str; 1] = ["admin"];
        map.insert(Commands::Admin as u32, &ADMIN);
        static NUKE: [&'static str; 1] = ["nuke"];
        map.insert(Commands::Nuke as u32, &NUKE);

        let memory = match Memory::load_from_file() {
            Ok(m) => m,

            Err(err) => {
                println!("[CRITICAL] Failed to load memory: {}", err);
                Memory { admin_roles: BTreeMap::new() }
            }
        };

        Ok(Box::new(Module {
                        commands: map,
                        memory: RwLock::new(memory),
                    }))
    }

    fn name(&self) -> &'static str {
        "Admin"
    }

    fn description(&self) -> &'static str {
        "Various management commands."
    }

    fn commands(&self) -> &HashMap<u32, &[&str]> {
        &self.commands
    }

    fn command_description(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Admin as u32 => "Manage the admin roles.",
            x if x == Commands::Nuke as u32 => "Deletes past messages.",
            _ => panic!("Admin::command_description - invalid id."),
        }
    }

    fn command_help_message(&self, id: u32) -> &'static str {
        match id {
            x if x == Commands::Admin as u32 => {
                "`!admin list` - Lists the roles who have access to the admin commands.\n\
                 `!admin add <role mention(-s)>` - Add a role (roles) to the admin roles.\n\
                 `!admin remove <role mention(-s)>` - Remove a role (roles) from the admin roles."
            }
            x if x == Commands::Nuke as u32 => "`!nuke <how many> [whose]` - Deletes the specified number of messages in the current channel. If any user mentions are present after the count, deletes the specified number of messages written by each of the people mentioned, and only theirs.",
            _ => panic!("Admin::command_help_message - invalid id."),
        }
    }

    fn handle(&self, bot: &Bot, message: &Message, id: u32, text: &str) {
        let state = bot.get_state().read().unwrap();

        let has_permission = match state.find_channel(message.channel_id) {
            Some(ChannelRef::Private(_)) => {
                bot.send(message.channel_id,
                         "Sorry, but you cannot use the admin commands through PMs. They don't make much sense here anyways.");
                return;
            }

            Some(ChannelRef::Public(server, _)) => {
                if message.author.id.0 == server.owner_id.0 {
                    true
                } else if let Some(admin_roles) = self.memory.read().unwrap().get_admin_roles(server.id) {
                    if let Ok(member) = bot.get_member(server.id, message.author.id) {
                        let mut found = false;

                        for role in member.roles {
                            if admin_roles.contains(&role.0) {
                                found = true;
                            }
                        }

                        found
                    } else {
                        bot.send(message.channel_id,
                                 "Sorry, I couldn't get your member info.");
                        return;
                    }
                } else {
                    false
                }
            }

            Some(ChannelRef::Group(_)) => {
                bot.send(message.channel_id, "Admin commands in groups? Hm.");
                return;
            }

            None => {
                bot.send(message.channel_id,
                         "Huh, I couldn't get this channel's info for some reason. Try again I guess?");
                return;
            }
        };

        if !has_permission {
            return;
        }

        match id {
            x if x == Commands::Admin as u32 => self.handle_admin(bot, message, text, state),
            x if x == Commands::Nuke as u32 => self.handle_nuke(bot, message, text),
            _ => panic!("Admin::handle - invalid id."),
        }
    }
}

impl<'a> Module<'a> {
    fn handle_admin(&self, bot: &Bot, message: &Message, text: &str, state: RwLockReadGuard<State>) {
        if let Some(caps) = ADMIN_REGEX.captures(&text.to_lowercase()) {
            // No need to recheck, we did that in handle().
            let server = match state.find_channel(message.channel_id).unwrap() {
                ChannelRef::Public(server, _) => server,
                _ => {
                    panic!("Did I just witness some memory corruption?");
                }
            };

            match caps.get(1).unwrap().as_str() {
                "list" => {
                    if let Some(admin_roles) = self.memory.read().unwrap().get_admin_roles(server.id) {
                        let mut buf = "Admin roles:".to_owned();

                        for role_id in admin_roles {
                            buf.push_str(&format!("\n- {} ", role_id));

                            buf.push_str(&if let Some(role) = server.roles.iter().filter(|x| x.id.0 == *role_id).next() {
                                             format!("`{}`", role.name)
                                         } else {
                                             " this role was removed".to_owned()
                                         });
                        }

                        bot.send(message.channel_id, &buf);
                    } else {
                        bot.send(message.channel_id, "There are no admin roles yet.");
                    }
                }

                "add" => {
                    if message.mention_roles.len() > 0 {
                        self.memory
                            .write()
                            .unwrap()
                            .add_admin_roles(server.id, &message.mention_roles);
                    } else {
                        bot.send(message.channel_id, "You didn't mention any roles.");
                    }
                }

                "remove" => {
                    if message.mention_roles.len() > 0 {
                        self.memory
                            .write()
                            .unwrap()
                            .remove_admin_roles(server.id, &message.mention_roles);
                    } else {
                        bot.send(message.channel_id, "You didn't mention any roles.");
                    }
                }

                _ => {
                    bot.send(message.channel_id,
                             <Module as module::Module>::command_help_message(&self, Commands::Admin as u32));
                }
            }
        } else {
            bot.send(message.channel_id,
                     <Module as module::Module>::command_help_message(&self, Commands::Admin as u32));
        }
    }

    fn handle_nuke(&self, bot: &Bot, message: &Message, text: &str) {
        if let Some(amount) = NUKE_REGEX
               .captures(text)
               .and_then(|x| x.get(2))
               .and_then(|x| x.as_str().parse::<u64>().ok())
               .map(|x| x + 1)
               .and_then(|x| if x <= 1 { None } else { Some(x) }) {
            let mentioned_user_ids: Vec<UserId> = message.mentions.iter().map(|x| x.id).collect();

            if let Ok(recent_message_ids) =
                bot.get_messages(message.channel_id, GetMessages::MostRecent, amount)
                    .map(|x| {
                             x.into_iter()
                                 .filter(|msg| mentioned_user_ids.len() == 0 || mentioned_user_ids.contains(&msg.author.id))
                                 .map(|msg| msg.id)
                                 .collect::<Vec<MessageId>>()
                         }) {
                bot.delete_messages(message.channel_id, &recent_message_ids);
            } else {
                bot.send(message.channel_id, "Error getting the recent messages.");
            }
        } else {
            bot.send(message.channel_id,
                     <Module as module::Module>::command_help_message(&self, Commands::Nuke as u32));
        }
    }
}
