#!/usr/bin/env python3

from telethon import TelegramClient, sync, types
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

        client = TelegramClient(config_name, cfg["api_id"], cfg["api_hash"]).start()
        channel = None
        for result in client.iter_dialogs(limit=None):
            if result.name == cfg["chat_url"]:
                channel = result.entity
                break
        assert channel

        get_inactive_users(sql, client, channel)


def user_name(u):
    username = u.username or ''
    if username:
        username = "(@%s)" % username
    return " ".join([u.first_name or '', u.last_name or '', username])


def user_status(u):
    if isinstance(u.status, types.UserStatusLastWeek):
        return "last week"
    if isinstance(u.status, types.UserStatusLastMonth):
        return "last month"
    if isinstance(u.status, types.UserStatusRecently):
        return "recently"
    if isinstance(u.status, types.UserStatusOnline):
        return "now"
    if isinstance(u.status, types.UserStatusOffline):
        return u.status.was_online.strftime('%Y-%m-%d')
    return "unknown"


def get_inactive_users(sql, client, channel):
    users = {}
    for user in client.get_participants(channel):
        users[str(user.id)] = user

    inactive_users = sql.execute("""
        select date(joined_on, 'unixepoch') as joined, id
            from known_users
            where removed_on is null
              and not exists
                (select 1 from comingouts where id = user_id)
            order by joined_on
    """).fetchall()

    for u in inactive_users:
        user = users[str(u["id"])]
        msgs = client.get_messages(
            channel,
            from_user=user,
            limit=10)

        res = [
            u["joined"],
            user_status(user),
            str(msgs.total - 1),
            str(user.id),
            user_name(user)
            ]
        print(";".join(res))
        for m in msgs:
            if isinstance(m, types.Message):
                print(";;;;;%s;%s" % (m.date.strftime('%Y-%m-%d'), m.message))
        time.sleep(1) # prevent limit-blocking

main()
