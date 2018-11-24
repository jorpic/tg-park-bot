#!/usr/bin/env python3

from telethon import TelegramClient, sync
import sqlite3
import sys


if len(sys.argv) != 3:
    print("Usage: %s <db.sqlite> <config_name>" % sys.argv[0])
    sys.exit()

db_path = sys.argv[1]
config_name = sys.argv[2]

with sqlite3.connect(sys.argv[1]) as sql:
    sql.row_factory = sqlite3.Row # column-indexable rows
    cfg = sql.execute("select * from bot_config where id=?", (config_name,))
    cfg = cfg.fetchone()

    client = TelegramClient(config_name, cfg["api_id"], cfg["api_hash"]).start()
    channel = client.get_entity(cfg["chat_url"])

    # update known_users
    users = client.get_participants(channel)
    sql.execute("""
        create temporary table current_users(
            id integer primary key
        )""")
    user_rows = [(u.id,) for u in users]
    sql.executemany("insert into current_users values (?)", user_rows)

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
    sql.execute("insert into sync_log values (strftime('%s', 'now'), null)")

# for building in [1,2,3,4]:
#     [m for m in client.get_messages(chan, search=("#%dкорпус" % building), limit=1000)]
