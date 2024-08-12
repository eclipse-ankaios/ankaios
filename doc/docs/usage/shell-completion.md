# Shell completion

Ankaios supports command completion for the `ank` CLI in various shells.

## Bash

Add the following lines to your `~/.bashrc`:

```shell
if command -v ank &> /dev/null; then
    source <(ank completion bash)
fi
```

## Z shell (zsh)

Add the following lines to your `~/.zshrc`:

```shell
if command -v ank &> /dev/null; then
    source <(ank completion zsh)
fi
```

## Fish

Add the following lines to your `~/.config/fish/config.fish`:

```shell
if type -q ank
    ank completion fish | source
end
```
