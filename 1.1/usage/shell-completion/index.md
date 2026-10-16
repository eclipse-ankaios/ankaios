# Shell completion

Ankaios supports command completion for the `ank` CLI in various shells.

> [!NOTE]
>
> Dynamic completion (workloads etc.) invokes the `ank` CLI internally and therefore requires the `ank` CLI to be able to connect to the Ankaios server (see [configuration files](<https://eclipse-ankaios.github.io/ankaios/1.1/reference/config-files/index.md>)).

## Bash

Add the following lines to your `~/.bashrc`:

```
if command -v ank &> /dev/null; then
    source <(COMPLETE=bash ank)
fi
```

## Z shell (zsh)

Add the following lines to your `~/.zshrc`:

```
if command -v ank &> /dev/null; then
    source <(COMPLETE=zsh ank)
fi
```

## Fish

Add the following lines to your `~/.config/fish/config.fish`:

```
if type -q ank
    source (COMPLETE=fish ank | psub)
end
```

## Elvish

```
echo "eval (COMPLETE=elvish ank)" >> ~/.elvish/rc.elv
```

## Powershell

```
echo "COMPLETE=powershell ank | Invoke-Expression" >> $PROFILE
```
