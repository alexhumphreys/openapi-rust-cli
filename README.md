# openapi-rust-cli

generate a cli client for a given openapi spec

## Try it out

```
docker-compose up                                       # start the test server
cargo run -- --config=openapi.yaml -h                   # see available commands
cargo run -- --config=openapi.yaml getUsers -h          # see arguments for a command
cargo run -- --config=openapi.yaml getUsers             # use one of those commands to hit the server
cargo run -- --config=openapi.yaml getUser 1            # use positional arguments for path params
cargo run -- --config=openapi.yaml getUsers --_limit=1   # use flags for query params
```

## How completion generation works
1. First, generate the completion script. Add this to your Rust app:
Replace `your_app_name` with your actual binary name
```
your_app_name generate-completions zsh > ~/.zsh/completion/_your_app_name
```

2. Create completion directory if it doesn't exist
```
mkdir -p ~/.zsh/completion
```

3. Add these lines to your ~/.zshrc:
```
fpath=(~/.zsh/completion $fpath)
autoload -U compinit
compinit
```

4. Reload your shell
```
source ~/.zshrc
```

Now you can use tab completion with your app!
Test it by typing your app name and pressing TAB:
```
your_app_name --[TAB]
```
