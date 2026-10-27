//! Trusted operator tool; never registered as desktop IPC.
fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    if args.len()==1 && args[0]=="--help" {
        println!("natra-sync-provision --install  (approved JSON packet via stdin)\nnatra-sync-provision --remove INSTALLATION_UUID LOCAL_USER_ID");
        return;
    }
    if natra_lib::sync::native_credentials::provision_cli(&args).is_ok() {
        println!("Credential store operation completed for this Windows account.");
    } else {
        eprintln!("Credential store operation failed. Review approved provisioning metadata and Windows account.");
        std::process::exit(1);
    }
}
