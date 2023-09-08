from robot.api.logger import info, debug, trace, console
import subprocess, os

def ank_bin_dir():
    ank_bin_path = os.environ["ANK_BIN_DIR"]
    if ank_bin_path.endswith("/"):
        return ank_bin_path
    
    return ank_bin_path + "/"

def start_server(startup_config):
    """
    Start Ankaios server
    """
    print("Start Ankaios server")
    args = [f"{ank_bin_dir()}ank-server", "--startup-config", startup_config]
    process = subprocess.Popen(args)
    
    yield

    # process.kill()


