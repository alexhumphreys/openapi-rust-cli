# 1. First, generate the completion script. Add this to your Rust app:
# Replace `your_app_name` with your actual binary name
your_app_name generate-completions zsh > ~/.zsh/completion/_your_app_name

# 2. Create completion directory if it doesn't exist
mkdir -p ~/.zsh/completion

# 3. Add these lines to your ~/.zshrc:
fpath=(~/.zsh/completion $fpath)
autoload -U compinit
compinit

# 4. Reload your shell
source ~/.zshrc

# Now you can use tab completion with your app!
# Test it by typing your app name and pressing TAB:
your_app_name --[TAB]
