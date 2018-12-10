#[macro_use]
extern crate failure;
extern crate futures;
extern crate sqlite;
extern crate telebot;
extern crate tokio_core;

use failure::Error;
use futures::stream::Stream;
use std::env;
use telebot::functions::*;
use telebot::RcBot;
use tokio_core::reactor::Core;

// FIXME: reuse prepared statements?


fn get_bot_key(sql: &sqlite::Connection, config: &str) -> Result<String, Error> {
    let mut query = sql.prepare("select bot_key from bot_config where id = ?")?;
    query.bind(1, config)?;

    match query.next()? {
        sqlite::State::Done => {
            Err(format_err!("No configuration found: {}", config))
        }
        sqlite::State::Row => Ok(query.read::<String>(0)?),
    }
}


fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: tg-park-bot <path to database> <config variant>");
        return Err(failure::err_msg("Invalid argument count!"));
    }

    let db_path = &args[1];
    let config_variant = &args[2];

    let sql = sqlite::open(db_path)?;
    let bot_key = get_bot_key(&sql, config_variant)?;

    let mut reactor = Core::new()?;
    let bot = RcBot::new(reactor.handle(), &bot_key).update_interval(500);
    let handle = bot.new_cmd("/start").and_then(move |(bot, msg)| {
        let mut chk_user = sql
            .prepare(
                "select 1 from known_users \
                where removed_on is null \
                  and joined_on < strftime('%s', 'now', '-2 days') \
                  and id = ?
                limit 1",
            )
            .unwrap();

        let user_id = msg.from.unwrap().id;
        chk_user.bind(1, user_id).unwrap();
        match chk_user.next() {
            Err(_err) => (), // TODO: add logger
            Ok(sqlite::State::Done) => {
                let text = "Простите, я вас не знаю.".to_string();
                bot.message(msg.chat.id, text).send();
            }
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
    bot.run(&mut reactor)?;
    Ok(())
}
