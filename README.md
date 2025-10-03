# Peak

Welcome to Peak! A little utility to navigate Git branches, quickly switch between them, and when you're done with them, delete them.

## Installation

TODO

## Usage

In a repository run Peak by running the `peak` command.

Then, in Peak, use the following features:

- [Switch to a branch](#switch-to-a-branch)
- [Filter branches](#filter-branches)
- [Display remote branches](#display-remote-branches)
- [Delete branches](#delete-branches)
- [Get help](#get-help)

### Switch to a branch

To switch (checkout) to a branch using Peak, navigate to the branch you want to check out with the arrow keys, or the `j` key for down and the `k` key for up (VIM keybindings). Press `Enter` to switch to the selected branch.

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
