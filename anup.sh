#!/usr/bin/env bash

EXE_NAME=$(realpath "$(dirname "$0")/anup")

# Check if the script is being ran in a terminal
tty -s

# Launch the program directly if we're in a terminal
if [ $? == 0 ]; then
    exec $EXE_NAME "$@"
fi

LAUNCH_TERM="xterm"

if [ "$TERMCMD" != "" ]; then
    LAUNCH_TERM=$TERMCMD
elif [ "$TERM" != "" ]; then
    LAUNCH_TERM=$TERM
fi

LAUNCH_FLAGS="-e"
# Some terminals don't work properly when the launch command is wrapped in quotes, so this
# is a hacky way to bypass them when necessary
QUOTE_CHAR="\""

if [ "$LAUNCH_TERM" == "gnome-terminal" ]; then
    LAUNCH_FLAGS="-- bash -c "
elif [ "$LAUNCH_TERM" == "alacritty" ]; then
    QUOTE_CHAR=""
fi

exec $LAUNCH_TERM $LAUNCH_FLAGS ${QUOTE_CHAR}sh -c ''$EXE_NAME' '"$@"'; read'${QUOTE_CHAR}