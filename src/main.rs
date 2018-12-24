#![feature(result_map_or_else)]

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate sqlite;
extern crate telebot;
extern crate tokio_core;

use failure::Error;
use futures::future::{self, Either};
use futures::{stream::Stream, Future};
use std::env;
use telebot::functions::*;
use telebot::objects::Message;
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

#[derive(Debug, PartialEq)]
enum UserStatus {
    Stranger,
    KnownButUntrusted,
    KnownAndTrusted,
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: tg-park-bot <path to database> <config variant>");
        return Err(failure::err_msg("Invalid argument count!"));
    }

    let db_path = &args[1];
    let config_variant = &args[2];

    env_logger::init();
    info!(
        "Started version '{}' with '{}' as DB and '{}' config",
        env!("GIT_HASH"),
        db_path,
        config_variant
    );

    let sql = sqlite::open(db_path)?;
    let bot_key = get_bot_key(&sql, config_variant)?;

    let mut reactor = Core::new()?;
    let bot = RcBot::new(reactor.handle(), &bot_key).update_interval(500);
    let handle = bot.new_cmd("/start").and_then(move |(bot, msg)| {
        let user_id = msg.from.unwrap().id;
        let chat_id = msg.chat.id;
        match is_known_user(&sql, user_id) {
            Err(err) => {
                error!("is_known_user: {:?}", err);
                Either::A(bot.message(
                    chat_id,
                    "Что-то пошло не так. \
                    Попробуйте ещё раз /start через некоторое время.".to_string(),
                ).send())
            },
            Ok(user_status) => {
                info!("new user joined: {} {:?}", user_id, user_status);
                Either::B(
                    greet_user(&user_status, bot, chat_id)
                    .and_then(move |(bot, msg)| either(
                        user_status != UserStatus::KnownAndTrusted,
                        (bot, msg),
                        future::ok,
                        move |(bot, _)| bot.message(chat_id, "OK".into()).send()
                        // TODO: поискать соседей
                        // TODO: предложить подписаться
                    ))
                )
            }
        }
    });

    bot.register(handle);
    bot.run(&mut reactor)?;
    Ok(())
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

fn greet_user(
    user_status: &UserStatus,
    bot: RcBot,
    chat_id: i64,
) -> impl Future<Item = (RcBot, Message), Error = Error> {
    let greeting = compose_greeting(user_status);
    bot.message(chat_id, greeting).send()
}

fn compose_greeting(user_status: &UserStatus) -> String {
    match user_status {
        UserStatus::Stranger => {
            "Простите, я вас не знаю.".to_string()
        },
        UserStatus::KnownButUntrusted => {
            format!(
                "Вы совсем недавно присоединились к нашему уютному чатику, \
                мне нужно время, чтобы узнать вас получше.\n{}",
                NEW_USER_MSG)
        },
        UserStatus::KnownAndTrusted => {
            "Привет! Я робот. Я могу помочь вам найти соседей.".to_string()
        }
    }
}

fn either<I, E, F, G, FnF, FnG>(
    cond: bool,
    x: I,
    f: FnF,
    g: FnG,
) -> Either<F, G>
where
    F: Future<Item = I, Error = E>,
    G: Future<Item = I, Error = E>,
    FnF: FnOnce(I) -> F,
    FnG: FnOnce(I) -> G,
{
    match cond {
        true => Either::A(f(x)),
        false => Either::B(g(x)),
    }
}
