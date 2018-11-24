#!/usr/bin/env bash

PRJ=tg-park-bot
SRV=pluto
PFX=/opt

HOOK=$PRJ.git/hooks/post-receive

ssh -T $SRV <<SSH
cd $PFX
mkdir $PRJ
git init --bare $PRJ.git
cat >> $HOOK <<HOOK
#!/usr/bin/env bash
git --work-tree=$PFX/$PRJ --git-dir=$PFX/$PRJ.git checkout -f
HOOK
chmod +x $HOOK
SSH

git remote add $SRV ssh://$SRV/$PFX/$PRJ.git
