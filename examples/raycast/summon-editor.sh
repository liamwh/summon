#!/bin/bash
# Summon Editor — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Editor
# @raycast.mode silent
# @raycast.description Open or focus Editor via Summon

export PATH="$HOME/bin:$PATH"
summon editor
