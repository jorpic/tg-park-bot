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
use telebot::RcBot;
use tokio_core::reactor::Core;

// FIXME: reuse prepared statements?

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

    let send = |bot: RcBot, msg, user_info: FullUserInfo, text| {
        bot.message(user_info.chat_id, text)
            .send()
            .map(|(bot, msg)| (bot, msg, user_info))
            .map_err(|err| (bot, msg, BotError::Fatal(err)))
    };

    let handle = bot.new_cmd("/start").then(move |args| {
        let (bot, msg) = args.expect("Fatal error");
        let user_id = msg.from.clone().unwrap().id; // FIXME: unwrap
        let chat_id = msg.chat.id;
        match get_full_user_info(&sql, user_id, chat_id) {
            Err(err) => Err((bot, msg, BotError::Fatal(err))),
            Ok(user_info) => {
                info!("/start: {:?}", user_info);
                Ok((bot, msg, user_info))
            }
        }
    })
    .and_then(move |(bot, msg, user_info)| {
        let text = compose_greeting(&user_info.status);
        send(bot, msg, user_info, text)
    })
    .and_then(move |(bot, msg, user_info)| {
        either(user_info.status != UserStatus::KnownAndTrusted,
            (bot, msg,  user_info),
            |(bot, msg, _)| future::err((bot, msg, BotError::Done)),
            |(bot, msg, user_info)| {
                let text = compose_places(&user_info.places);
                send(bot, msg, user_info, text)
            })
    })
    .and_then(|(bot, msg, _)| future::ok((bot, msg)))
    .or_else(|(bot, msg, err)| {
        match err {
            BotError::Done => Either::A(future::ok((bot, msg))),
            BotError::Fatal(err) => {
                error!("msg.from: {:?}, {:?}", msg.from, err);
                Either::B(bot.message(
                    msg.chat.id,
                    "Что-то пошло не так. \
                    Попробуйте ещё раз /start через некоторое время.".to_string(),
                ).send())
            }
        }
    });

    // TODO: рассказать про теги
    // TODO: поискать соседей
    // TODO: предложить подписаться

    bot.register(handle);
    bot.run(&mut reactor)?;
    Ok(())
}

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

enum BotError {
    Done,
    Fatal(Error),
}

#[derive(Debug)]
struct FullUserInfo {
    id: i64,
    chat_id: i64,
    status: UserStatus,
    places: Vec<PlaceToLive>,
    neighbors: Vec<NeighborMessage>,
}

#[derive(Debug)]
struct NeighborMessage {}

#[derive(Debug)]
struct PlaceToLive {
    pub building: i64,
    pub floor: i64,
}

fn get_full_user_info(
    sql: &sqlite::Connection,
    user_id: i64,
    chat_id: i64,
) -> Result<FullUserInfo, Error> {
    let status = is_known_user(&sql, user_id)?;
    let places = where_they_live(&sql, user_id)?;
    let neighbors = Vec::new();
    Ok(FullUserInfo {
        id: user_id,
        chat_id,
        status,
        places,
        neighbors,
    })
}

fn where_they_live(
    sql: &sqlite::Connection,
    user_id: i64,
) -> Result<Vec<PlaceToLive>, Error> {
    let mut query = sql.prepare(
        "select distinct building_num, floor_num
        from comingouts
        where deprecated = 0 and user_id = ?",
    )?;
    query.bind(1, user_id)?;
    let mut res = Vec::new();
    while let sqlite::State::Row = query.next()? {
        let building = query.read::<i64>(0)?;
        let floor = query.read::<i64>(1)?;
        res.push(PlaceToLive { building, floor });
    }
    Ok(res)
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
    let mut res = UserStatus::Stranger;

    while let sqlite::State::Row = query.next()? {
        match query.read::<i64>(0)? {
            0 => res = UserStatus::KnownButUntrusted,
            _ => res = UserStatus::KnownAndTrusted,
        }
    }
    Ok(res)
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

fn compose_places(places: &[PlaceToLive]) -> String {
    match places.len() {
        0 => "Я не знаю где вы живёте.\nЧтобы это исправить, вам нужно \
            в чатик ЖК отправить сообщение вида #Xкорпус #Yэтаж. Например \
            '#3корпус #11этаж'. Минут через пять после этого возвращайтесь \
            и ещё раз нажмите /start.".to_string(),
        1 => format!(
            "Похоже, что вы живёте в {}-м корпусе на {}-м этаже.",
            places[0].building, places[0].floor),
        _ => "Какая неожиданность. Похоже вы отправили несколько сообщений \
            с указанием своего этажа. Теперь я не знаю как быть. \
            Попробуйте связаться с @MaxTaldykin".to_string(),
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
