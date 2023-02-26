use super::events::EventType;
use addresses::Address;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

macro_rules! scope_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$variant_meta:meta])* $variant_name:ident,)*
    }) => {
        paste::paste! {
            $(#[$meta])*
            $vis enum $name {
                $($(#[$variant_meta])* $variant_name([<$variant_name Scope>])),*
            }
            impl std::convert::From<EventType> for $name {
                fn from(value: EventType) -> Self {
                    match value {
                        $(EventType::$variant_name => $name::$variant_name(kaspa_notify::scope::[<$variant_name Scope>]::default())),*
                    }
                }
            }
        }
    }
}

scope_enum! {
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Scope {
    BlockAdded,
    VirtualSelectedParentChainChanged,
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged,
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct BlockAddedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualSelectedParentChainChangedScope {
    pub include_accepted_transaction_ids: bool,
}

impl VirtualSelectedParentChainChangedScope {
    pub fn new(include_accepted_transaction_ids: bool) -> Self {
        Self { include_accepted_transaction_ids }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictScope {}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictResolvedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct UtxosChangedScope {
    pub addresses: Vec<Address>,
}

impl UtxosChangedScope {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self { addresses }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualSelectedParentBlueScoreChangedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualDaaScoreChangedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PruningPointUtxoSetOverrideScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct NewBlockTemplateScope {}
