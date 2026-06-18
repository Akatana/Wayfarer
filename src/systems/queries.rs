use crate::command::ClientId;
use crate::components::{
    ClientConnection, InInventory, ItemName, Name, NpcId, Position, RoomContents,
};

/// Finds the first item on the floor of `room_id` whose name contains `name_lower`.
pub fn find_item_in_room(
    world: &hecs::World,
    room_id: u64,
    name_lower: &str,
) -> Option<(hecs::Entity, String)> {
    let mut q = world.query::<(&ItemName, &RoomContents)>();
    q.iter()
        .find(|(_, (n, rc))| rc.room_id == room_id && n.0.to_lowercase().contains(name_lower))
        .map(|(e, (n, _))| (e, n.0.clone()))
}

/// Finds the first item in `owner`'s bag whose name contains `name_lower`.
pub fn find_item_in_inventory(
    world: &hecs::World,
    owner: hecs::Entity,
    name_lower: &str,
) -> Option<(hecs::Entity, String)> {
    let mut q = world.query::<(&ItemName, &InInventory)>();
    q.iter()
        .find(|(_, (n, inv))| inv.owner == owner && n.0.to_lowercase().contains(name_lower))
        .map(|(e, (n, _))| (e, n.0.clone()))
}

/// Finds the first NPC in `room_id` whose name contains `name_lower`.
/// Returns `(entity, name, npc_db_id)`.
pub fn find_npc_in_room(
    world: &hecs::World,
    room_id: u64,
    name_lower: &str,
) -> Option<(hecs::Entity, String, i64)> {
    let mut q = world.query::<(&Name, &Position, &NpcId)>();
    q.iter()
        .find(|(_, (n, pos, _))| pos.room_id == room_id && n.0.to_lowercase().contains(name_lower))
        .map(|(e, (n, _, npc_id))| (e, n.0.clone(), npc_id.0))
}

/// Returns the `ClientId` of every player in `room_id` except `exclude`.
pub fn clients_in_room_except(
    world: &hecs::World,
    room_id: u64,
    exclude: hecs::Entity,
) -> Vec<ClientId> {
    let mut q = world.query::<(&Position, &ClientConnection)>();
    q.iter()
        .filter(|(e, (p, _))| *e != exclude && p.room_id == room_id)
        .map(|(_, (_, conn))| conn.client_id)
        .collect()
}
