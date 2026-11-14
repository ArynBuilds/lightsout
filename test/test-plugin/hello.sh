#!/bin/sh
# A plugin's command: what was written after its name comes as LIGHTSOUT_ARG, and it talks
# back to Lights Out through the command line.
echo "hello $LIGHTSOUT_ARG from $LIGHTSOUT_COMMAND" >> "$LIGHTSOUT_PLUGIN_STATE/out.txt"
lightsout notify "hello $LIGHTSOUT_ARG"
