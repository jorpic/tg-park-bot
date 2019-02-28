#!/usr/bin/env python3

from telethon import TelegramClient, sync, functions
import re
import sys
import sqlite3
import time


def main():
    if len(sys.argv) != 3:
        print("Usage: %s <db.sqlite> <config_name>" % sys.argv[0])
        sys.exit()

    db_path = sys.argv[1]
    config_name = sys.argv[2]
    log("Program started")

    sql = sqlite3.connect(sys.argv[1])
    sql.row_factory = sqlite3.Row # enable column-indexable rows
    cfg = sql.execute("select * from bot_config where id=?", (config_name,))
    cfg = cfg.fetchone()

    client = TelegramClient(config_name, cfg["api_id"], cfg["api_hash"]).start()
    channel = None
    try:
        channel = client.get_entity(cfg["chat_url"])
    except:
        # Wow, couldn't find channel by url.
        # This may be the case when channel is private, try to find it by
        # name.
        for result in client.iter_dialogs(limit=None):
            if result.name == cfg["chat_url"]:
                channel = result.entity
                break
    assert channel

    bot = client.get_entity(cfg["bot_username"])
    assert bot.bot
    sql.close()

    while True:
        with sqlite3.connect(sys.argv[1]) as sql:
            sql.row_factory = sqlite3.Row # enable column-indexable rows
            log("Loop")
            update_known_users(sql, client, channel)
            get_new_messages(sql, client, channel)
            sql.execute("insert into sync_log values (strftime('%s', 'now'), null)")

            forward_new_messages(sql, client, channel, bot)
        time.sleep(5*60) # sleep 5 minutes


def log(msg):
    now = time.strftime("%Y-%m-%d %H:%M:%S")
    print("%s: %s" % (now, msg) , flush=True)


def update_known_users(sql, client, channel):
    users = client.get_participants(channel)
    sql.execute("""
        create temporary table current_users(
            id integer primary key,
            name text not null
        )""")

    def name(u):
        username = u.username or ''
        if username:
            username = "(@%s)" % username
        return " ".join([u.first_name or '', u.last_name or '', username])

    user_rows = [(u.id, name(u)) for u in users]
    sql.executemany("insert into current_users values (?, ?)", user_rows)

    # add missing users
    sql.execute("""
        insert into known_users
            select *, strftime('%s', 'now'), null from current_users c
            where not exists
                (select 1 from known_users k where k.id = c.id)
    """)

    # mark removed users
    sql.execute("""
        update known_users as k
            set removed_on = strftime('%s', 'now')
            where not exists
                (select 1 from current_users c where c.id = k.id)
    """)

    # rejoin returning users
    sql.execute("""
        update known_users as k
            set removed_on = null, joined_on = strftime('%s', 'now')
            where removed_on is not null
              and exists (select 1 from current_users c where c.id = k.id)
    """)
    sql.execute("drop table current_users")


def get_new_messages(sql, client, channel):
    rx = re.compile(r'#(\d+)этаж',  re.IGNORECASE)
    msg_rows = []
    for building in [1,2,3,4]:
        msgs = client.get_messages(
            channel,
            search=("#%dкорпус" % building),
            limit=1000)
        for m in msgs:
            floors = rx.findall(m.message)
            if len(floors) > 0:
                msg_rows.append((
                    m.id,
                    m.from_id,
                    m.date.strftime("%s"),
                    m.message,
                    building,
                    floors[0]))

    sql.executemany("""
        insert or ignore into comingouts
            ( msg_id, user_id, msg_date, msg_text
            , building_num, floor_num
            , deprecated)
            values (?,?,?,?,?,?,0)
        """,
        msg_rows)

    # skip some erroneous messages
    sql.execute("""
        update comingouts
            set deprecated = 1
            where (msg_id = 11258 and user_id = 796267776)
               or (msg_id = 9729 and user_id = 125290876)
    """)


def forward_new_messages(sql, client, channel, bot):
    messages = sql.execute("""
        select * from comingouts where forwarded_msg_id is null
    """).fetchall()

    for row in messages:
        log("msg_id: %s txt: %s" % (row["msg_id"], row["msg_text"]))
        client(functions.messages.ForwardMessagesRequest(
            from_peer=channel,
            id=[row["msg_id"]],
            to_peer=bot
        ))
        time.sleep(1) # prevent limit-blocking

main()
