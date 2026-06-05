#!/bin/bash
# Summon WezTerm — Raycast Script Command
#
# Install: copy to ~/.config/raycast/scripts/ (or your Raycast scripts directory)
# Requires: summon in PATH
#
# @raycast.schemaVersion 1
# @raycast.title Summon WezTerm
# @raycast.mode silent
# @raycast.description Open or focus WezTerm via Summon

export PATH="$HOME/bin:$PATH"
summon wezterm
