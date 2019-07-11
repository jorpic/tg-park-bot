#!/usr/bin/env python3
# -*- encoding: utf-8 -*-

from telethon import TelegramClient, sync, functions, types
import sys
import sqlite3
import time

def main():
    if len(sys.argv) != 3:
        print("Usage: %s <db.sqlite> <config_name>" % sys.argv[0])
        sys.exit()

    db_path = sys.argv[1]
    config_name = sys.argv[2]

    with sqlite3.connect(sys.argv[1]) as sql:
        sql.row_factory = sqlite3.Row # enable column-indexable rows
        cfg = sql.execute("select * from bot_config where id=?", (config_name,))
        cfg = cfg.fetchone()

        src = None
        for result in client.iter_dialogs(limit=None):
            if result.name == cfg["chat_url"]:
                src = result.entity
                break
        dest = None
        for result in client.iter_dialogs(limit=None):
            if result.name == "Жители первого корпуса":
                dest = result.entity
                break
        assert src
        assert dest

        messages = sql.execute("""
            select user_id, max(msg_id) as msg_id
              from comingouts inner join known_users u
                on user_id = u.id
                  and building_num = 1
                  and not deprecated
                  and removed_on is null
              group by user_id
        """).fetchall()

        for row in messages:
            print("user: %s msg: %s" % (row["user_id"], row["msg_id"]))
            client(functions.messages.ForwardMessagesRequest(
                from_peer=src,
                id=[row["msg_id"]],
                to_peer=dest
            ))
            time.sleep(1) # prevent limit-blocking
