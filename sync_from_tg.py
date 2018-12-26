#!/usr/bin/env python3

from telethon import TelegramClient, sync
import sqlite3
import sys
import re


if len(sys.argv) != 3:
    print("Usage: %s <db.sqlite> <config_name>" % sys.argv[0])
    sys.exit()

db_path = sys.argv[1]
config_name = sys.argv[2]

with sqlite3.connect(sys.argv[1]) as sql:
    sql.row_factory = sqlite3.Row # enable column-indexable rows
    cfg = sql.execute("select * from bot_config where id=?", (config_name,))
    cfg = cfg.fetchone()

    client = TelegramClient(config_name, cfg["api_id"], cfg["api_hash"]).start()
    channel = client.get_entity(cfg["chat_url"])

    ### update known_users
    users = client.get_participants(channel)
    sql.execute("""
        create temporary table current_users(
            id integer primary key,
            name text not null
        )""")

    def name(u):
        username = u.username
        if username:
            username = "(@%s)" % username
        " ".join([u.first_name or '', u.last_name or '', username or ''])

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


    ### add disclosure messages
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

    sql.executemany(
            "insert or ignore into comingouts values (?,?,?,?,?,?)",
            msg_rows)

    sql.execute("insert into sync_log values (strftime('%s', 'now'), null)")
