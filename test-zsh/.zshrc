# Alacritty Math Test Environment

# OSC 133
precmd() { print -Pn "\e]133;A\a" }
preexec() { print -Pn "\e]133;C\a" }

PS1='%F{green}[math]%f %~ %# '

echo -E "Math test ready. Try: echo -E '\$\\alpha + \\frac{a}{b}\$'"
