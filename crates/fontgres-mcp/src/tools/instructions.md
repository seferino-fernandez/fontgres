# FONTGRES MCP Server

This server exposes the FONTGRES API as tools.

- `fontgres_get_system_status`: server health.
- `fontgres_list_items`: list items (paginated: `page`, `page_size`).
- `fontgres_get_item`: fetch one item by `id`.
- `fontgres_create_item`: create an item (`name`, `enabled`).
- `fontgres_delete_item`: delete an item by `id`. Destructive and cannot be undone.

Every tool rejects an argument it does not declare, so send only the listed
arguments.

Destructive tools may ask for confirmation through MCP form elicitation. Retry
the call with the same arguments, the returned request state and the user's
answer. This confirmation is a best-effort courtesy, not access control.

Replace these example tools with your API's operations.
