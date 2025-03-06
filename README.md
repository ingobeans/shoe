# shoe

![screenshot](https://github.com/user-attachments/assets/119bd733-ddd0-443d-8736-fa88ca2f2fb5)

shoe: a shell!

i made this as a basic replacement to the built-in shells in windows, as cmd can be really dumb and powershell can be really frustrating (and slow as hell on old machines).

i'm adding the features i find necessary, and trying to make it look pretty!

i also ensured all builtin commands are simple but powerful. an example: to copy a file to a new path, thats `cp source dest`, and to copy an entire directory recursively to a new path, thats also `cp source dest`. you dont need any special flags to seperate the two cases, the copy command will copy what you tell it to.

## install

`cargo install --git https://github.com/ingobeans/shoe.git`

## features

- running commmands! (both builtin and executables)
- piping commands, redirecting output to files, etc
- using ~ in paths to cd to and tab autocomplete
- persistent command history (stored at ~/.shoehistory)
- show inline suggestions (from history) which can be completed by pressing right arrow at the end of the line (like in powershell)
- rc file (at ~/.shoerc)

## special characters

- `;` or `&` - seperates two commands
- `&&` - seperates two commands and only runs the seconds one if the first one succeeds
- `||` - seperates two commands and only runs the seconds one if the first one fails
- `>` - writes the output of the command to a file at the following path
- `<` - reads stdin to the command from a file at the following path
- `|` - pipes the output of a command to the next's stdin
- `\` - escapes a special character
- `"` - you can enclose an argument in quotes
