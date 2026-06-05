#!/bin/bash
# Summon Preview — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Preview
# @raycast.mode silent
# @raycast.description Open or focus Preview via Summon

export PATH="$HOME/bin:$PATH"
summon preview
