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

fn get_bot_key(
    sql: &sqlite::Connection,
    config: &str,
) -> Result<String, Error> {
    let mut query =
        sql.prepare("select bot_key from bot_config where id = ?")?;
    query.bind(1, config)?;

    match query.next()? {
        sqlite::State::Done => {
            Err(format_err!("No configuration found: {}", config))
        }
        sqlite::State::Row => Ok(query.read::<String>(0)?),
    }
}

// We prevent new chat members from accessing neighbourhood information.
const NEW_USER_TIMEOUT: &str = "-2 days";
const NEW_USER_MSG: &str =
    "Возвращайтесь через пару дней.";

enum UserStatus {
    Stranger,
    KnownButUntrusted,
    KnownAndTrusted,
}

fn is_known_user(
    sql: &sqlite::Connection,
    user_id: i64,
) -> Result<UserStatus, Error> {
    let mut query = sql.prepare(
        "select joined_on < strftime('%s', 'now', ?) from known_users \
         where removed_on is null and id = ? \
         limit 1",
    )?;

    query.bind(1, NEW_USER_TIMEOUT)?;
    query.bind(2, user_id)?;
    match query.next()? {
        sqlite::State::Done => Ok(UserStatus::Stranger),
        sqlite::State::Row => match query.read::<i64>(0)? {
            0 => Ok(UserStatus::KnownButUntrusted),
            _ => Ok(UserStatus::KnownAndTrusted),
        },
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
        let user_id = msg.from.unwrap().id;
        let chat_id = msg.chat.id;
        is_known_user(&sql, user_id).map(|known| match known {
            UserStatus::Stranger => {
                let text = "Простите, я вас не знаю.".to_string();
                bot.message(chat_id, text).send()
            },
            UserStatus::KnownButUntrusted => {
                let text = format!(
                    "Вы совсем недавно присоединились к нашему чатику, \
                    мне нужно время, чтобы узнать вас получше.\n{}",
                    NEW_USER_MSG);
                bot.message(chat_id, text).send()
            },
            UserStatus::KnownAndTrusted => {
                // TODO: поискать соседей
                // TODO: предложить подписаться
                let text = "Привет!".to_string();
                bot.message(chat_id, text).send()
            }
        })
    });

    bot.register(handle);
    bot.run(&mut reactor)?;
    Ok(())
}
