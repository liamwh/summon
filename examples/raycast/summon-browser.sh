#!/bin/bash
# Summon Browser — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Browser
# @raycast.mode silent
# @raycast.description Open or focus Browser via Summon

export PATH="$HOME/bin:$PATH"
summon browser
