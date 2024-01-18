use leptos::{component, create_memo, For, IntoView, RwSignal, SignalGet, SignalUpdate, SignalWith, SignalWithUntracked, view};
use tracing::debug;

use crate::components::toast::notification::Notification;
use crate::components::toast::ToastMap;

#[component]
pub fn Container(
    toasts: RwSignal<ToastMap>
) -> impl IntoView {

    debug!("Creating toast container.");

    let remove_toast = move |key| {
        toasts.update(|toasts| {
            toasts.remove(key);
        });
    };

    let toasts_list = create_memo(move |_| {
        let mut result = toasts.with(|toasts| {
            toasts.iter()
                .map(|(key, toast)| (key, Clone::clone(toast)))
                .collect::<Vec<_>>()
        });
        result.sort_unstable_by(|(_, lhs), (_, rhs)| {
            lhs.with_untracked(|lhs| {
               rhs.with_untracked(|rhs| {
                   lhs.timestamp.cmp(&rhs.timestamp)
               })
            })
        });
        result
    });

    view! {
        <div class="dut-toasts">
            <For
                each=move || toasts_list.get()
                key=|(key, _)| *key
                children=move |(key, toast)| {
                    view! {
                        <Notification
                            toast
                            on_remove=move || remove_toast(key)
                        />
                    }
                }
            />
        </div>
    }
}
