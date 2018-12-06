extern crate futures;
extern crate telebot;
extern crate tokio_core;

use futures::stream::Stream;
use futures::Future;
use std::env;
use telebot::RcBot;
use tokio_core::reactor::Core;

use telebot::functions::*;

const TG_KEY_VAR: &str = "TELEGRAM_BOT_KEY";
// const DB_PATH_VAR: &str = "DB_PATH";


fn main() {
    let bot_key = env::var(TG_KEY_VAR)
        .expect(&format!("Env variable is not set: {}", TG_KEY_VAR));
//    let db_path =  env::var(DB_PATH_VAR)
//        .expect(&format!("Env variable is not set: {}", DB_PATH_VAR));

    let mut reactor = Core::new().unwrap();
    let bot = RcBot::new(reactor.handle(), &bot_key).update_interval(500);

    let handle = bot.new_cmd("/start").and_then(|(bot, msg)| {
        println!("From user {}. ChatId {}", msg.from.unwrap().id, msg.chat.id);
        Ok(())
    });

    bot.register(handle);
    bot.run(&mut reactor).unwrap();
}
