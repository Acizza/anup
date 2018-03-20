#!/bin/bash

EXE_NAME="$(dirname "$0")/anitrack"

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

if [ $LAUNCH_TERM == "gnome-terminal" ]; then
    exec $LAUNCH_TERM -- bash -c "$EXE_NAME "$@" && read"
else
    exec $LAUNCH_TERM -e "$EXE_NAME "$@""
fi