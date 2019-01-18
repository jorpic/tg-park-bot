
create table bot_config(
  id text primary key,
  chat_url text not null,
  api_id text not null,
  api_hash text not null,
  bot_username text not null,
  bot_key text not null
);


create table sync_log(
  sync_date datetime not null,
  error_msg text
);


create table known_users(
  id integer primary key,
  name text not null,
  joined_on datetime not null,
  removed_on datetime
);


create table comingouts(
  msg_id integer primary key,
  user_id integer not null,
  msg_date datetime not null,
  msg_text text not null,
  building_num integer not null,
  floor_num integer not null,
  forwarded_msg_id integer,
  forwarded_chat_id integer,
  deprecated boolean not null default 0,

  foreign key(user_id) references known_users(id)
);
