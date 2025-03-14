#!/bin/bash
printf "\nChecking computer_actions.rs for syntax errors...\n"
rustc -Z no-codegen --crate-type=lib --edition=2021 src/computer_actions.rs || echo "Found syntax errors"
