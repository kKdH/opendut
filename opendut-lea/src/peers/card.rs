use leptos::prelude::*;
use opendut_types::peer::state::PeerConnectionState;
use crate::app::use_app_globals;
use crate::peers::components::CreatePeerButton;
use crate::routing;

#[derive(Clone)]
struct Peers {
    online: usize,
    offline: usize,
}

#[component]
pub fn PeersCard() -> impl IntoView {

    let globals = use_app_globals();

    let peers: LocalResource<Peers> = LocalResource::new(move || {
        let mut carl = globals.client.clone();
        async move {
            let peer_states = carl.peers.list_peer_states().await
                .expect("Failed to request the list of peer states.");

            let (online, offline): (Vec<_>, Vec<_>) = peer_states.values()
                .partition(|peer_state| match peer_state.connection {
                    PeerConnectionState::Online { .. } => true,
                    PeerConnectionState::Offline => false,
                });

            Peers {
                offline: offline.len(),
                online: online.len(),
            }
        }
    });

    view! {
        <div class="card">
            <div class="card-header">
                <a class="card-header-title has-text-link" href=routing::path::peers_overview>"Peers"</a>
            </div>
            <div class="card-content">
                <div class="level">
                    <div class="level-item has-text-centered">
                        <div>
                            <p class="heading">Online</p>
                            <p class="title">
                                <Suspense
                                    fallback={ move || view! { <span>"-"</span> }}
                                >
                                    <span>{ move || peers.get().map(|peers| peers.online) }</span>
                                </Suspense>
                            </p>
                        </div>
                    </div>
                    <div class="level-item has-text-centered">
                        <div>
                            <p class="heading">Offline</p>
                            <p class="title">
                                <Suspense
                                    fallback={ move || view! { <span>"-"</span> }}
                                >
                                    <span>{ move || peers.get().map(|peers| peers.offline) }</span>
                                </Suspense>
                            </p>
                        </div>
                    </div>
                </div>
            </div>
            <div class="card-footer">
                <div class="m-2">
                    <CreatePeerButton />
                </div>
            </div>
        </div>
    }
}
