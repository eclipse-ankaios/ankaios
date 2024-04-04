# Requirement description template

All requirements in Ankaios shall be written in the following format:

```markdown
<Requirement title>
`swdd~<component>-<descriptive requirement id>~<version>`

Status: approved

[When <condition separated by and>], <object> shall <do something | be in state | execute a list of actions in order/in parallel | …>

Comment:
<comment body>

Rationale:
<rationale body>

Tags:
- <tag1>
- <tag2>
- …

Needs:
- [impl/utest/stest]
```

NOTE:

* Only one object is allowed.
* Format is markdown.
* Tags are the objects and subject of the requirement, which are already specified in the structural view.
* Requirements shall use "shall" and not "should", "will", "is" etc.

Here is an example of the requirement from the Ankaios agent:

```markdown
#### AgentManager listens for requests from the Server
`swdd~agent-manager-listens-requests-from-server~1`

Status: approved

The AgentManager shall listen for request from the Server.

Tags:
- AgentManager

Needs:
- impl
- utest
- itest
```

This requirement template has been inspired by:

<https://aaltodoc.aalto.fi/server/api/core/bitstreams/d518c3cc-4d7d-4c69-b7db-25d2da9e847f/content>
