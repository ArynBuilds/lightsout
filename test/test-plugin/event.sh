#!/bin/sh
# Runs when a room is made: the event's name and what it carries.
echo "$LIGHTSOUT_EVENT $LIGHTSOUT_EVENT_JSON" >> "$LIGHTSOUT_PLUGIN_STATE/events.txt"
