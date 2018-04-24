#!/usr/bin/env bash

EXE_NAME=$(realpath "$(dirname "$0")/tranim")

# Check if the script is being ran in a terminal
tty -s

# Launch the program directly if we're in a terminal
if [ $? == 0 ]; then
    exec $EXE_NAME "$@"
fi

# Read custom environment variables to get the default terminal
source ~/.bash_profile

LAUNCH_TERM="xterm"

if [ $TERMCMD != "" ]; then
    LAUNCH_TERM=$TERMCMD
elif [ $TERM != "" ]; then
    LAUNCH_TERM=$TERM
fi

LAUNCH_FLAGS="-e"

if [ $LAUNCH_TERM == "gnome-terminal" ]; then
    LAUNCH_FLAGS="-- bash -c "
fi

exec $LAUNCH_TERM $LAUNCH_FLAGS "$EXE_NAME "$@""