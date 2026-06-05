#!/bin/bash
# Summon Finder — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Finder
# @raycast.mode silent
# @raycast.description Open or focus Finder via Summon

export PATH="$HOME/bin:$PATH"
summon finder
