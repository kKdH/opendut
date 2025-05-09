use leptos::either::Either;
use leptos::prelude::*;

use crate::api::ApiError;
use crate::api::ComponentLicenses;
use crate::components::BasePageContainer;
use crate::components::LoadingSpinner;

#[component]
pub fn LicensesOverview() -> impl IntoView {

    let licenses: LocalResource<Result<Vec<ComponentLicenses>, ApiError>> = LocalResource::new(move || {
        async move {
            crate::api::get_licenses().await
        }
    });

    let license_information = Suspend::new(async move {
        match licenses.await {
            Ok(components) => {
                let components = components.iter()
                    .map(|component| {
                        let component_name = Clone::clone(&component.name);
                        let rows = component.licenses.iter().map(|dependency| {
                            let dependency_name = Clone::clone(&dependency.name);
                            let dependency_version = Clone::clone(&dependency.version);
                            let dependency_licenses = dependency.licenses.join(", ");
                            view! {
                                <tr>
                                    <td>{ dependency_name }</td>
                                    <td>{ dependency_version }</td>
                                    <td>{ dependency_licenses }</td>
                                </tr>
                            }
                        }).collect::<Vec<_>>();

                        view! {
                            <div class="message">
                                <div class="message-header">
                                    <p class="is-family-monospace is-uppercase">{component_name}</p>
                                </div>
                                <div class="message-body">
                                    <table class="table is-hoverable is-fullwidth">
                                        <thead>
                                            <tr>
                                                <th class="">"Dependency"</th>
                                                <th class="">"Version"</th>
                                                <th class="">"License"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            { rows }
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        }
                    })
                    .collect::<Vec<_>>();

                Either::Right(view! {
                    <div>{ components }</div>
                })
            }
            Err(_) => {
                Either::Left(view! {
                    <div class="notification is-warning is-light">
                        <p>"No license information available."</p>
                    </div>
                })
            }
        }
    });

    view! {
        <BasePageContainer
            title="Licenses"
            breadcrumbs=Vec::new()
            controls=view! { <> }
        >
            <div class="mt-4">
                <Transition
                    fallback=LoadingSpinner
                >
                    { license_information }
                </Transition>
            </div>
        </BasePageContainer>
    }
}
