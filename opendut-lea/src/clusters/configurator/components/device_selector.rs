use std::collections::HashSet;
use std::ops::Not;

use leptos::prelude::*;
use leptos::reactive::wrappers::write::SignalSetter;

use opendut_types::peer::{PeerDescriptor, PeerId};
use opendut_types::topology::{DeviceDescriptor, DeviceId};
use opendut_types::util::net::NetworkInterfaceDescriptor;
use crate::clusters::configurator::components::get_all_selected_devices;
use crate::clusters::configurator::types::UserClusterConfiguration;
use crate::components::{ButtonColor, ButtonSize, ButtonState, FontAwesomeIcon, IconButton};
use crate::util::{Ior, NON_BREAKING_SPACE};
use crate::util::net::UserNetworkInterfaceConfiguration;

pub type DeviceSelectionError = String;
pub type DeviceSelection = Ior<DeviceSelectionError, HashSet<DeviceId>>;

#[component]
pub fn DeviceSelector(
    cluster_configuration: RwSignal<UserClusterConfiguration>,
    peers: ReadSignal<Vec<PeerDescriptor>>,
) -> impl IntoView {

    let (getter, setter) = create_slice(
        cluster_configuration,
        |config| Clone::clone(&config.devices),
        |config, input| {
            config.devices = input;
        },
    );

    let selected_devices = move || get_all_selected_devices(getter);

    let help_text = move || {
        getter.with(|selection| match selection {
            DeviceSelection::Right(_) => String::from(NON_BREAKING_SPACE),
            DeviceSelection::Left(error) => error.to_owned(),
            DeviceSelection::Both(error, _) => error.to_owned(),
        })
    };

    fn render_peer_descriptors(peer_descriptors: Vec<PeerDescriptor>, selected_devices: HashSet<DeviceId>, getter: Signal<DeviceSelection>, setter: SignalSetter<DeviceSelection>) -> impl IntoView {
            let mut all_devices_by_peer: Vec<_> = Vec::new();

            for peer in peer_descriptors {
                let mut devices = peer.topology.devices;

                devices.sort_by(|a, b|
                a.name.value().to_lowercase().cmp(&b.name.value().to_lowercase()));

                let interfaces_and_devices = peer.network.interfaces_zipped_with_devices(&devices);

                let devices_per_peer = interfaces_and_devices.into_iter()
                    .map(|(network_interface, device)| {
                        let collapsed_signal = RwSignal::new(true);
                        let collapse_button_icon = Signal::derive(move || if collapsed_signal.get() { FontAwesomeIcon::ChevronDown } else {FontAwesomeIcon::ChevronUp} );
                        let selected_signal = RwSignal::new(selected_devices.contains(&device.id));
                        view! {
                            <tr>
                                <td class="is-narrow">
                                    <IconButton
                                        icon=collapse_button_icon
                                        color=ButtonColor::White
                                        size=ButtonSize::Small
                                        state=ButtonState::Enabled
                                        label="Show or hide device details"
                                        on_action=move || collapsed_signal.update(|collapsed| *collapsed = collapsed.not())
                                    />
                                </td>
                                <td>
                                    {device.name.to_string()}
                                </td>
                                <td>{peer.location.clone().unwrap_or_default().to_string()}</td>
                                <td class="is-narrow">
                                    <IconButton
                                        icon=FontAwesomeIcon::Check
                                        color=Signal::derive(move || match selected_signal.get() {
                                            false => ButtonColor::Light,
                                            true => ButtonColor::Success,
                                        })
                                        size=ButtonSize::Small
                                        state=ButtonState::Enabled
                                        label="More infos"
                                        on_action=move || icon_button_on_action(
                                            selected_signal,
                                            getter,
                                            setter,
                                            device.id,
                                        )
                                    />
                                </td>
                            </tr>
                            <tr hidden={collapsed_signal}>
                                <DeviceInfo
                                    device = device
                                    network_interface = network_interface
                                    peer_id = peer.id
                                />
                            </tr>
                        }
                    }).collect_view();

                for device in devices_per_peer {
                    all_devices_by_peer.push(device);
                }
            }
            all_devices_by_peer
    }

    view! {
        <p class="help has-text-danger">{ help_text }</p>
        <div class="table-container mt-2">
            <table class="table is-fullwidth">
                <thead>
                    <tr>
                        <th></th>
                        <th>Name</th>
                        <th>Peer Location</th>
                        <th></th>
                    </tr>
                </thead>
                    <tbody>
                        { move || render_peer_descriptors(peers.get(), selected_devices(), getter, setter) }
                    </tbody>
            </table>
        </div>
    }
}

#[component]
pub fn DeviceInfo(device: DeviceDescriptor, network_interface: NetworkInterfaceDescriptor, peer_id: PeerId) -> impl IntoView {
    view! {
        <td></td>
        <td colspan="3">
            <div class="field">
                <label class="label">ID</label>
                <div class="control">
                    <p>{device.id.to_string()}</p>
                </div>
            </div>
            <div class="field">
                <label class="label">Peer ID</label>
                <div class="control">
                    <p>{peer_id.to_string()}</p>
                </div>
            </div>
            <div class="field">
                <label class="label">Interface</label>
                <div class="control">
                    <p>{network_interface.name.name()} " (" {UserNetworkInterfaceConfiguration::from(network_interface.configuration).display_name()} ")"</p>
                </div>
            </div>
            <div class="field">
                <label class="label">Tags</label>
                <div class="control">
                    <p>{device.tags.iter().map(|tag| tag.value()).collect::<Vec<_>>().join("* ")}</p>
                </div>
            </div>
            <div class="field">
                <label class="label">Description</label>
                <div class="control">
                    <p>{device.description.unwrap_or_default().to_string()}</p>
                </div>
            </div>
        </td>
    }
}

pub fn icon_button_on_action(
    selected_signal: RwSignal<bool>,
    getter: Signal<DeviceSelection>,
    setter: SignalSetter<DeviceSelection>,
    device_id: DeviceId,
) {
    selected_signal.update(|selected| *selected = selected.not());
    let insert = selected_signal.get();
    let device_selection = match getter.get() {
        DeviceSelection::Left(error) => {
            if insert {
                Ior::Both(
                    String::from("Select at least one more device."),
                    HashSet::from([device_id]),
                )
            } else {
                Ior::Left(error)
            }
        }
        DeviceSelection::Right(mut devices) | DeviceSelection::Both(_, mut devices) => {
            if insert {
                devices.insert(device_id);
            } else {
                devices.remove(&device_id);
            }
            match devices.len() {
                0 => Ior::Left(String::from("Select at least two devices.")),
                1 => Ior::Both(String::from("Select at least one more device."), devices),
                _ => Ior::Right(devices),
            }
        }
    };
    setter.set(device_selection);
}
