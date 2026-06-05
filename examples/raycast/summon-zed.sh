#!/bin/bash
# Summon Zed — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon Zed
# @raycast.mode silent
# @raycast.description Open or focus Zed via Summon

export PATH="$HOME/bin:$PATH"
summon zed
