# Shell completion

Ankaios supports command completion for the `ank` CLI in various shells.

!!! note

    For dynamic completion (workloads etc.) to work, the `ank` CLI must be configured via environment variables.
    To use a non-default server URL, provide `ANK_SERVER_URL`.
    Also provide either `ANK_INSECURE=true` or `ANK_CA_PEM`, `ANK_CRT_PEM` and `ANK_KEY_PEM`.

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
