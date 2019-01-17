#!/usr/bin/env python3

from telethon import TelegramClient, sync, functions
import sqlite3
import sys
import time


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
    bot = client.get_entity(cfg["bot_username"])
    assert bot.bot

    messages = sql.execute("""
        select msg_id from comingouts where owned_msg_id is null
    """).fetchall()

    for (msg_id,) in messages:
        print(msg_id)
        client(functions.messages.ForwardMessagesRequest(
            from_peer=channel,
            id=[msg_id],
            to_peer=bot
        ))
        time.sleep(1) # prevent limit-blocking
