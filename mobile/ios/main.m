// iPad shell: everything runs in Rust (crates/ezyx-mobile). ezyx_ios_main starts winit's
// UIApplicationMain on this thread and only returns if the app could not start.
extern int ezyx_ios_main(void);

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    return ezyx_ios_main();
}
