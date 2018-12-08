extern crate futures;
extern crate tokio_core;
extern crate telebot;
extern crate sqlite;

use std::env;
use futures::stream::Stream;
use tokio_core::reactor::Core;
use telebot::RcBot;
use telebot::functions::*;


const TG_KEY_VAR: &str = "TELEGRAM_BOT_KEY";
const DB_PATH_VAR: &str = "DB_PATH";


fn env_var(key: &str) -> String {
    match env::var(key) {
        Ok(val) => val,
        Err(err) => panic!("Env variable {} is not properly set: {}", key, err)
    }
}


fn main() {
    let bot_key = env_var(TG_KEY_VAR);
    let db_path = env_var(DB_PATH_VAR);

    let sql = sqlite::open(db_path).unwrap();

    let mut reactor = Core::new().unwrap();
    let bot = RcBot::new(reactor.handle(), &bot_key).update_interval(500);
    let handle = bot.new_cmd("/start").and_then(move |(bot, msg)| {
        // FIXME: reuse prepared statement
        let mut chk_user = sql.prepare(
            "select 1 from known_users \
                where removed_on is null \
                  and joined_on < strftime('%s', 'now', '-2 days') \
                  and id = ?
                limit 1").unwrap();

        let user_id = msg.from.unwrap().id;
        chk_user.bind(1, user_id).unwrap();
        match chk_user.next() {
            Err(err) => (), // TODO: add logger
            Ok(sqlite::State::Done) => {
                let text = "Простите, я вас не знаю.".to_string();
                bot.message(msg.chat.id, text).send();
            },
            Ok(sqlite::State::Row) => {
                // TODO: поискать соседей
                // TODO: предложить подписаться
                let text = "Hi!".to_string();
                bot.message(msg.chat.id, text).send();
            }
        }

        Ok(())
    });

    bot.register(handle);
    bot.run(&mut reactor).unwrap();
}
