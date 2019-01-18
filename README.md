# 🤖

> This little bot knows where you live and wants to share this information with
> all your neighbours!


Jokes aside, we care about your privacy. We store only information that you
explicitly want to share with your neighbours. Bot does not talk to strangers,
only trusted members of the group chat can get information from it.

## ⚙️ How it works

Once in a while bot updates his knowledge by scanning the group chat:
- list current members, remove banned ones;
- search for tagged messages and forward them to the bot (see note below).

When human approaches, bot is on his duty to help:
- check if human is known and trusted; 
- check if human has revealed where they live;
- forward messages from neighbours.

### Notes
Telegram is obsessed with your privacy, so our innocent bot is not allowed to forward
messages directly from group chat. To overcome this limit we log in as a human member
of the group and forward tagged messages to the bot, now bot owns them.
