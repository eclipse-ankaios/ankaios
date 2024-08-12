# Shell completion

Ankaios supports command completion for the `ank` CLI in various shells.

## Bash

Add the following lines to your `~/.bashrc`:

```bash
if command -v ank &> /dev/null; then
    source <(ank complete --shell bash --register -)
fi
```

## Z shell (zsh)

Add the following lines to your `~/.zshrc`:

```zsh
if command -v ank &> /dev/null; then
    source <(ank complete --shell zsh --register -)
fi
```

## Fish

Add the following lines to your `~/.config/fish/config.fish`:

```fish
if type -q ank
    source (ank complete --shell fish --register - | psub)
end
```

## Elvish

```elvish
echo "eval (ank complete --shell elvish --register -)" >> ~/.elvish/rc.elv
```

## Powershell

```powershell
echo "ank complete --shell powershell --register - | Invoke-Expression" >> $PROFILE
```
