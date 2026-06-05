#!/bin/bash
# Summon Notion — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Notion
# @raycast.mode silent
# @raycast.description Open or focus Notion via Summon

export PATH="$HOME/bin:$PATH"
summon notion
