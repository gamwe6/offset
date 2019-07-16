@0xe34d46ee2fe7213e;

using import "common.capnp".Signature;
using import "common.capnp".PublicKey;
using import "common.capnp".HashResult;
using import "common.capnp".RandValue;
using import "common.capnp".Uid;
using import "common.capnp".CustomUInt128;
using import "common.capnp".Rate;

using import "funder.capnp".FriendsRoute;

# IndexClient <-> IndexServer
###################

struct Edge {
        fromPublicKey @0: PublicKey;
        toPublicKey @1: PublicKey;
}

# IndexClient -> IndexServer
struct RequestRoutes {
        requestId @0: Uid;
        capacity @1: CustomUInt128;
        source @2: PublicKey;
        destination @3: PublicKey;
        optExclude: union {
                empty @4: Void;
                edge @5: Edge;
        }
}


struct RouteCapacityRate {
        route @0: FriendsRoute;
        capacity @1: CustomUInt128;
        rate @2: Rate;
}

struct MultiRoute {
        routes @0: List(RouteCapacityRate);
}

# IndexServer -> IndexClient
struct ResponseRoutes {
        requestId @0: Uid;
        multiRoutes @1: List(MultiRoute);
}

struct UpdateFriend {
        publicKey @0: PublicKey;
        # Friend's public key
        sendCapacity @1: CustomUInt128;
        # To denote remote requests closed, assign 0 to sendCapacity
        recvCapacity @2: CustomUInt128;
        # To denote local requests closed, assign 0 to recvCapacity
        rate @3: Rate;
        # Rate a node takes for forwarding messages for this friend (to another
        # node).
}


# IndexClient -> IndexServer
struct IndexMutation {
        union {
                updateFriend @0: UpdateFriend;
                removeFriend @1: PublicKey;
        }
}

struct MutationsUpdate {
        nodePublicKey @0: PublicKey;
        # Public key of the node sending the mutations.
        indexMutations @1: List(IndexMutation);
        # List of mutations to relationships with direct friends.
        timeHash @2: HashResult;
        # A time hash (Given by the server previously).
        # This is used as time, proving that this message was signed recently.
        sessionId @3: Uid;
        # A randomly generated sessionId. The counter is related to this session Id.
        counter @4: UInt64;
        # Incrementing counter, making sure that mutations are received in the correct order.
        # For a new session, the counter should begin from 0 and increment by 1 for every MutationsUpdate message.
        # When a new connection is established, a new sesionId should be randomly generated.
        randNonce @5: RandValue;
        # Rand nonce, used as a security measure for the next signature.
        signature @6: Signature;
        # signature(sha_512_256("MUTATIONS_UPDATE") ||
        #           nodePublicKey ||
        #           mutation ||
        #           timeHash ||
        #           counter ||
        #           randNonce)
}

struct TimeProofLink {
        hashes @0: List(HashResult);
        # List of hashes that produce a certain hash
        # sha_512_256("TIME_HASH" || hashes)
}

struct ForwardMutationsUpdate {
        mutationsUpdate @0: MutationsUpdate;
        timeProofChain @1: List(TimeProofLink);
        # A proof that MutationsUpdate was signed recently
        # Receiver should verify:
        # - sha_512_256(hashes[0]) == MutationsUpdate.timeHash,
        # - For all i < n - 1 : hashes[i][index[i]] == sha_512_256(hashes[i+1])
        # - hashes[n-1][index[n-1]] is some recent time hash generated by the receiver.
}

###################################################

struct IndexServerToClient {
        union {
                timeHash @0: HashResult;
                responseRoutes @1: ResponseRoutes;
        }
}


struct IndexClientToServer {
        union {
                mutationsUpdate @0: MutationsUpdate;
                requestRoutes @1: RequestRoutes;
        }
}


struct IndexServerToServer {
        union {
                timeHash @0: HashResult;
                forwardMutationsUpdate @1: ForwardMutationsUpdate;
        }
}
