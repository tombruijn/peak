# Git Brancher

Welcome to Brancher! A small little utility to navigate branches, quickly switch between them, and when you're done with them, delete them.

## Installation

TODO

## Usage

In a repository run Brancher by running the `brancher` command.

Then, in Brancher, use the following features:

- [Switch to a branch](#switch-to-a-branch)
- [Filter branches](#filter-branches)
- [Display remote branches](#display-remote-branches)
- [Delete branches](#delete-branches)
- [Get help](#get-help)

### Switch to a branch

To switch (checkout) to a branch using Brancher, navigate to the branch you want to check out with the arrow keys, or the `j` key for down and the `k` key for up (VIM keybindings).

### Filter branches

To filter the list of branches, use the following steps:

- Press `f` or `/` to open the filter.
- Enter the characters which you want to filter on, for example `feature/`, and press `Enter` to confirm.
- To reset the filter, press `R`.

### Display remote branches

To also list the known remote branches, since the last fetch, press the `t` key to 'toggle' the types of branches listed.

### Delete branches

To delete branches, use the following steps:

- Press `x` to mark a branch for deletion.
- Then press `d` to delete them. You'll be asked to confirm before they're deleted.

### Get help

- To see a reference of key mappings press `h` for the help screen.
- Press `Esc` or `q` to return to the main view.

## License

Copyright (c) Tom de Bruijn <tom@tomdebruijn.com>

This project is licensed under the MIT license ([LICENSE] or <http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
