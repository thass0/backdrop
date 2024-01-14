#!/usr/bin/env bash

# This script will automatically install and optionally run
# an instance of backdrop in a local folder on your machine.

set -eo pipefail

# Check whether git is installed on the host machine.
if ! [ -x "$(command -v git)" ]; then
  echo >&2 "Error: git is not installed."
  echo >&2 "Please install git for your system (https://git-scm.com/downloads)"
  echo >&2 "Try running this script again after you have successfully installed git."
  exit 1
fi

# Clone the git repository and cd into it.
git clone https://github.com/thass0/backdrop.git || { echo >&2 "Git clone failed with $?"; exit 1; }
cd backdrop

# Exit now if the user does not want to run an instance.

# Language agnostic y/n promt copied from stack overflow: https://stackoverflow.com/a/226724
set -- $(locale LC_MESSAGES)
yesexpr="$1"; noexpr="$2"; yesword="$3"; noword="$4"

while true; do
  read -p "Do you want to run a backdrop instance now? (${yesword}/${noword}) " yn
  if [[ "$yn" =~ $yesexpr ]]; then
    # Make the script for running a local instance executable and run it.
    chmod +x scripts/run_local.sh
    bash scripts/run_local.sh
    exit
  fi
  if [[ "$yn" =~ $noexpr ]]; then exit; fi
done
