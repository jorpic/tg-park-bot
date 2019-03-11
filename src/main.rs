#![deny(clippy::pedantic)]
#![allow(clippy::non_ascii_literal)]

use failure::{format_err, Error};
use futures::future::{self, Either, IntoFuture};
use futures::stream::{iter_ok, Stream};
use futures::Future;
use log::{error, info};
use std::env;
use telebot::functions::*;
use telebot::objects::Message;
use telebot::RcBot;
use tokio_core::reactor::Core;

// We prevent new chat members from accessing neighbourhood information.
const NEW_USER_TIMEOUT: &str = "-2 hours";
const NEW_USER_MSG: &str =
    "Возвращайтесь через пару часов.";

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
    bot.register(start_cmd(sql, &bot));

    // TODO: explain why simultaneous db connections are safe here
    let sql = sqlite::open(db_path)?;
    let stream = bot.get_stream().and_then(move |(_, upd)| {
        if let Some(msg) = upd.message {
            update_comingout(&sql, &msg)?;
        }
        Ok(())
    });

    reactor.run(stream.for_each(|_| Ok(())).into_future())?;
    Ok(())
}

fn update_comingout(
    sql: &sqlite::Connection,
    msg: &Message,
) -> Result<(), Error> {
    if let (Some(user), Some(text)) = (&msg.forward_from, &msg.text) {
        let mut query = sql.prepare(
            "update comingouts
            set forwarded_chat_id = ?,
                forwarded_msg_id = ?
            where forwarded_chat_id is null
              and forwarded_msg_id is null
              and user_id = ?
              and msg_text = ?",
        )?;
        query.bind(1, msg.chat.id)?;
        query.bind(2, msg.message_id)?;
        query.bind(3, user.id)?;
        query.bind(4, &text[..])?;
        query.next()?;
        info!("Update forwarded msg {} from {}", msg.message_id, user.id);
    }
    Ok(())
}

fn start_cmd(sql: sqlite::Connection, bot: &RcBot) -> impl Stream {
    bot.new_cmd("/start")
    .then(move |args| {
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
        let text = match user_info.status {
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
                "Привет! Я робот. Могу помочь вам найти соседей.".to_string()
            }
        };
        send(bot, msg, user_info, text)
    })
    .and_then(stop_if(|user| user.status != UserStatus::KnownAndTrusted))
    .and_then(move |(bot, msg, user_info)| {
        let text = match user_info.places.len() {
            0 => "Увы, не знаю, где вы живёте.\nЧтобы это исправить, вам нужно \
                в чатик ЖК отправить сообщение вида #Xкорпус #Yэтаж. Например \
                '#3корпус #11этаж'. Минут через пять после этого возвращайтесь \
                и ещё раз нажмите /start.".to_string(),
            1 => format!(
                "Похоже, что вы живёте в {}-м корпусе на {}-м этаже.",
                user_info.places[0].building, user_info.places[0].floor),
            _ if user_info.is_landlord =>
                "Оу, я вижу вы занимаете сразу несколько этажей.".to_string(),
            _ => "Какая неожиданность. Похоже вы отправили несколько сообщений \
                с указанием своего этажа. Теперь я не знаю как быть. \
                Попробуйте написать в общий чатик.".to_string(),
        };
        send(bot, msg, user_info, text)
    })
    .and_then(stop_if(|user| user.places.len() != 1 && !user.is_landlord))
    .and_then(move |(bot, msg, user_info)| {
        let text = match user_info.neighbors.len() {
            0 => "Я не знаю ваших соседей, мне очень жаль. \
                 Попробуйте зайти ещё когда-нибудь и снова нажать /start.",
            _ => "У вас есть соседи. Сейчас перешлю их сообщения.",
        }.to_string();
        send(bot, msg, user_info, text)
    })
    .and_then(stop_if(|user| user.neighbors.is_empty()))
    .and_then(|(bot, msg, user_info)| {
        let bot_copy = bot.clone();
        forward_many(bot, user_info.chat_id, user_info.neighbors)
            .map_err(move |err| (bot_copy, msg, BotError::Fatal(err)))
    })
    // .and_then(|(bot, msg, _)| future::ok((bot, msg)))
    .or_else(|(bot, msg, err)| {
        match err {
            BotError::Done => Either::A(future::ok((bot, msg))),
            BotError::Fatal(err) => {
                error!("msg.from: {:?}, {:?}", msg.from, err);
                Either::B(bot.message(
                    msg.chat.id,
                    "Что-то пошло не так. \
                    Попробуйте ещё раз /start через некоторое время.\n \
                    Или пожалуйтесь в общий чатик.".to_string(),
                ).send())
            }
        }
    })
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
    is_landlord: bool,
}

fn get_full_user_info(
    sql: &sqlite::Connection,
    user_id: i64,
    chat_id: i64,
) -> Result<FullUserInfo, Error> {
    let status = is_known_user(&sql, user_id)?;
    let places = where_they_live(&sql, user_id)?;
    let is_landlord = is_landlord(&sql, user_id)?;
    let neighbors = if places.len() > 0 {
        get_neighbors(&sql, user_id)?
    } else {
        Vec::new()
    };
    Ok(FullUserInfo {
        id: user_id,
        chat_id,
        status,
        places,
        neighbors,
        is_landlord,
    })
}

#[derive(Debug, PartialEq)]
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
    let mut res = UserStatus::Stranger;
    while let sqlite::State::Row = query.next()? {
        match query.read::<i64>(0)? {
            0 => res = UserStatus::KnownButUntrusted,
            _ => res = UserStatus::KnownAndTrusted,
        }
    }
    Ok(res)
}

#[derive(Debug)]
struct PlaceToLive {
    pub building: i64,
    pub floor: i64,
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

#[derive(Debug, Clone)]
struct NeighborMessage {
    pub chat_id: i64,
    pub msg_id: i64,
}

fn get_neighbors(
    sql: &sqlite::Connection,
    user_id: i64,
) -> Result<Vec<NeighborMessage>, Error> {
    let mut query = sql.prepare(
        "with
          floors(usr, bdg, flr) as (
            select user_id, building_num, floor_num
              from comingouts
              where not deprecated
                and user_id = ?),
          neighbor_floors as (
            select distinct *
              from floors
                union select usr, bdg, flr-1 from floors
                union select usr, bdg, flr+1 from floors),
          neighbors_messages(usr, flr, msg) as (
            select user_id, floor_num, max(msg_id)
              from known_users u, comingouts c, neighbor_floors f
              where u.id = c.user_id
                and u.id <> f.usr
                and c.building_num = f.bdg
                and c.floor_num = f.flr
                and removed_on is null
                and not deprecated
                and forwarded_chat_id is not null
                and forwarded_msg_id is not null
              group by user_id, floor_num)
          select forwarded_chat_id, forwarded_msg_id
            from comingouts
              join neighbors_messages on (user_id = usr and msg_id = msg)
            order by floor_num, user_id",
    )?;
    query.bind(1, user_id)?;
    let mut res = Vec::new();
    while let sqlite::State::Row = query.next()? {
        res.push(NeighborMessage {
            chat_id: query.read::<i64>(0)?,
            msg_id: query.read::<i64>(1)?,
        });
    }
    Ok(res)
}

fn is_landlord(sql: &sqlite::Connection, user_id: i64) -> Result<bool, Error> {
    let mut query =
        sql.prepare("select is_landlord from known_users where id = ?")?;
    query.bind(1, user_id)?;
    let mut res = false;
    while let sqlite::State::Row = query.next()? {
        res = res || query.read::<i64>(0)? == 1;
    }
    Ok(res)
}

#[allow(dead_code)] // See https://github.com/rust-lang/rust/issues/18290
type PipeArg = (RcBot, Message, FullUserInfo);
#[allow(dead_code)]
type PipeErr = (RcBot, Message, BotError);

fn forward_many<I>(
    bot: RcBot,
    chat_id: i64,
    msgs: I,
) -> impl Future<Item = (RcBot, Message), Error = Error>
where
    I: IntoIterator<Item = NeighborMessage>,
{
    iter_ok(msgs)
        .fold((bot, None), move |(bot, _), n| {
            bot.forward(chat_id, n.chat_id, n.msg_id)
                .send()
                .map(|(bot, msg)| (bot, Some(msg)))
        })
        .map(|(bot, msg)| (bot, msg.unwrap()))
}

fn send(
    bot: RcBot,
    msg: Message,
    user_info: FullUserInfo,
    text: String,
) -> impl Future<Item = PipeArg, Error = PipeErr> {
    bot.message(user_info.chat_id, text)
        .send()
        .map(|(bot, msg)| (bot, msg, user_info))
        .map_err(|err| (bot, msg, BotError::Fatal(err)))
}

fn stop_if<F>(predicate: F) -> impl FnMut(PipeArg) -> Result<PipeArg, PipeErr>
where
    F: Fn(&FullUserInfo) -> bool,
{
    move |(bot, msg, user_info)| {
        if predicate(&user_info) {
            Err((bot, msg, BotError::Done))
        } else {
            Ok((bot, msg, user_info))
        }
    }
}
