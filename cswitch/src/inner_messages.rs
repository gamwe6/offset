extern crate num_bigint;

use self::num_bigint::BigInt;

// TODO: Find a good crypto library to use for short signatures.

const UID_LEN: usize = 16;
const PUBLIC_KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 32;

struct Uid([u8; UID_LEN]);

// Helper structs
// --------------

struct PublicKey([u8; PUBLIC_KEY_LEN]);
struct Signature([u8; SIGNATURE_LEN]);

struct ChannelerAddress {
    // TODO: Ipv4 or Ipv6 address
}

struct NeighborInfo {
    neighbor_public_key: PublicKey,
    neighbor_address: ChannelerAddress,
    max_channels: u32,  // Maximum amount of token channels
    token_channel_capacity: u64,    // Capacity per token channel
}

struct NeighborTokenChannel {
    mutual_credit: i64, // TODO: Is this the right type? 
    // Possibly change to bignum?
    
    // TODO:
    // - A type for last token
    // - Keep last operation?

}

struct NeighborTokenChannelInfo {
    neighbor_info: NeighborInfo,
    neighbor_token_channels: Vec<NeighborTokenChannel>,
}

struct NeighborsRoute {
    route: Vec<u32>,
}

struct FriendsRoute {
    route: Vec<u32>,
    // How much credit can we push through this route?
    capacity: u64,
}



// Channeler to Networker
// ----------------------

struct ChannelOpened {
    channel_uid: Uid,
    remote_public_key: PublicKey, // Public key of remote side
    locally_initialized: bool, // Was this channel initiated by this end.
}

struct ChannelClosed {
    channel_uid: Uid,
}


struct ChannelMessageReceived {
    channel_uid: Uid,
    message_content: Vec<u8>,
}

struct ResponseNeighborsRelationList {
    neighbors_relation_list: Vec<NeighborInfo>,
}

enum ChannelerToNetworker {
    ChannelOpened(ChannelOpened),
    ChannelClosed(ChannelClosed),
    ChannelMessageReceived(ChannelMessageReceived),
    ResponseNeighborsRelationList(ResponseNeighborsRelationList),
}


// Networker to Channeler
// ----------------------

enum NetworkerToChanneler {
    SendChannelMessage {
        channel_uid: Uid,
        message_content: Vec<u8>,
    },
    CloseChannel {
        channel_uid: Uid,
    },
    AddNeighborRelation {
        neighbor_info: NeighborInfo,
    },
    RemoveNeighborRelation {
        neighbor_public_key: PublicKey,
    },
    SetMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
    RequestNeighborsRelationList,
}

// Indexer client to Networker
// ---------------------------


enum IndexerClientToNetworker {
    RequestSendMessage {
        request_id: Uid,
        request_content: Vec<u8>,
        neighbors_route: NeighborsRoute,
        max_response_length: u64,
        processing_fee: u64,
        delivery_fee: u64,
        dest_node_public_key: PublicKey,
    },
    IndexerAnnounce {
        content: Vec<u8>,
    },
}

// Networker to Indexer client
// ---------------------------

enum ResponseSendMessageContent {
    Success(Vec<u8>),
    Failure,
}

enum NotifyStructureChangeNeighbors {
    NeighborAdded(PublicKey),
    NeighborRemoved(PublicKey),
}

enum NetworkerToIndexerClient {
    ResponseSendMessage {
        request_id: Uid,
        content: ResponseSendMessageContent,
    },
    NotifyStructureChange(NotifyStructureChangeNeighbors),
    MessageReceived {
        source_node_public_key: PublicKey,
        message_content: Vec<u8>,
    },
}


// Networker to Plugin Manager
// ---------------------------


enum NetworkerToPluginManager {
    SendMessageRequestReceived {
        request_id: Uid,
        source_node_public_key: PublicKey,
        request_content: Vec<u8>,
        max_response_length: u64,
        processing_fee: u64,
    },
    InvalidNeighborMoveToken {
        // TODO
    },
    ResponseNeighborsList {
        neighbors_list: Vec<NeighborTokenChannelInfo>,
    },

    NeighborUpdates {
        // TODO: Add some summary of information about neighbors and their token channels.
        // Possibly some counters?
    },
}

// Plugin Manager to Networker
// ---------------------------


enum PluginManagerToNetworker { 
    RespondSendMessageRequest {
        request_id: Uid,
        response_content: Vec<u8>,
    },
    DiscardSendMessageRequest {
        request_id: Uid,
    },
    SetNeighborChannelCapacity {
        token_channel_capacity: u64,    // Capacity per token channel
    },
    SetNeighborMaxChannels {
        max_channels: u32,
    },
    AddNeighbor {
        neighbor_info: NeighborInfo,
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    RequestNeighborsList,
    IndexerAnnounceSelf {
        current_time: u64,      // Current perceived time
        owner_public_key: PublicKey, // Public key of the signing owner.
        owner_sign_time: u64,   // The time in which the owner has signed
        owner_signature: Signature, // The signature of the owner over (owner_sign_time || indexer_public_key)
    },
}

// Networker to Pather
// -------------------

enum NetworkerToPather {
    ResponseSendMessage {
        request_id: Uid,
        content: ResponseSendMessageContent,
    },
}

// Pather to Networker
// -------------------

enum PatherToNetworker {
    RequestSendMessage {
        request_id: Uid,
        request_content: Vec<u8>,
        neighbors_route: NeighborsRoute,
        max_response_length: u64,
        processing_fee: u64,
        delivery_fee: u64,
    },
}

// Indexer Client to Pather
// ------------------------

enum IndexerClientToPather {
    ResponseNeighborsRoute {
        routes: Vec<NeighborsRoute>
    }
}

// Pather to Indexer client
// ------------------------

enum PatherToIndexerClient {
    RequestNeighborsRoute {
        source_node_public_key: PublicKey,
        dest_node_public_key: PublicKey,
    }
}

// Pather to Plugin Manager
// ------------------------

enum PatherToPluginManager {
    ResponseOpenPath {
        request_id: Uid,
        path_id: Uid,
    },
    ResponsePathSendMessage {
        request_id: Uid,
        content: ResponseSendMessageContent,
    },
}

// Plugin Manager to Pather
// ------------------------

enum PluginManagerToPather {
    RequestOpenPath {
        request_id: Uid,
        dest_node_public_key: PublicKey,
    },
    ClosePath {
        path_id: Uid,
    },
    RequestPathSendMessage {
        request_id: Uid,
        path_id: Uid,
        request_content: Vec<u8>,
        max_response_length: u64,
        processing_fee: u64,
    }
}

// Pather to Funder
// ----------------

enum PatherToFunder {
    ResponseOpenPath {
        request_id: Uid,
        path_id: Uid,
    },
    ResponsePathSendMessage {
        request_id: Uid,
        content: ResponseSendMessageContent,
    },
}

// Funder to Pather
// ----------------

enum FunderToPather {
    RequestOpenPath {
        request_id: Uid,
        dest_node_public_key: PublicKey,
    },
    ClosePath {
        path_id: Uid,
    },
    RequestPathSendMessage {
        request_id: Uid,
        path_id: Uid,
        request_content: Vec<u8>,
        max_response_length: u64,
        processing_fee: u64,
    }
}

// Funder to Networker
// -------------------

enum ResponseSendFundStatus {
    Success,
    Failure,
}

enum FunderToNetworker {
    FundReceived {
        source_node_public_key: PublicKey,
        amount: u64,
        message_content: Vec<u8>,
    },
    ResponseSendFund {
        request_id: Uid,
        status: ResponseSendFundStatus,
    }
}


// Networker to Funder
// -------------------

enum NetworkerToFunder {
    MessageReceived {
        source_node_public_key: PublicKey,
        message_content: Vec<u8>,
    },
    RequestSendFund {
        request_id: Uid,
        amount: u64,
        message_content: Vec<u8>,
        dest_node_public_key: PublicKey,
    },
}


// Funder to Indexer Client
// ------------------------

struct FriendCapacity {
    friend_public_key: PublicKey,
    mutual_credit: BigInt,
    max_mutual_credit: BigInt,
    min_mutual_credit: BigInt,
}

enum NotifyStructureChangeFriends {
    FriendAdded(PublicKey),
    FriendRemoved(PublicKey),
    FriendCapacity(FriendCapacity),
}

enum RequestFriendsRoute {
    Direct {
        source_node_public_key: PublicKey,
        dest_node_public_key: PublicKey,
    },
    LoopFromFriend {
        // A loop from myself through given friend, back to myself.
        // This is used for money rebalance when we owe the friend money.
        friend_public_key: PublicKey,
    },
    LoopToFriend {
        // A loop from myself back to myself through given friend.
        // This is used for money rebalance when the friend owe us money.
        friend_public_key: PublicKey,
    },

}

enum FunderToIndexerClient {
    RequestFriendsRoute(RequestFriendsRoute),
    NotifyStructureChange(NotifyStructureChangeFriends),
}


// Indexer Client to Funder
// ------------------------

enum IndexerClientToFunder {
    ResponseFriendsRoute {
        routes: Vec<FriendsRoute>,
    },
}



