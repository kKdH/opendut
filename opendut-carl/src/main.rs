use opendut_carl::app_info;

shadow_formatted_version::from_shadow!(app_info);

const BANNER: &str = r"
                         _____     _______
                        |  __ \   |__   __|
   ___  _ __   ___ _ __ | |  | |_   _| |
  / _ \| '_ \ / _ \ '_ \| |  | | | | | |
 | (_) | |_) |  __/ | | | |__| | |_| | |
  \___/| .__/ \___|_| |_|_____/ \__,_|_|
       | |   _____          _____  _
       |_|  / ____|   /\   |  __ \| |
           | |       /  \  | |__) | |
           | |      / /\ \ |  _  /| |
           | |____ / ____ \| | \ \| |____
            \_____/_/    \_\_|  \_\______|

              - He Fixes the Cable -";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("{BANNER}\n{version_info}", version_info=FORMATTED_VERSION);

    opendut_carl::create_with_telemetry(opendut_util::settings::Config::default()).await
}
