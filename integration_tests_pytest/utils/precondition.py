from utils.formatters import table_to_dict
import time

class Precondition:

    def __init__(self, ank_cli):
        self.__ank_cli = ank_cli

    def wait_for_initial_execution_state(self, timeout=10):
        start_time = time.time()
        _, table = self.__ank_cli.run("get workload", format_func=table_to_dict)
        while (time.time() - start_time) < timeout:
            if all(len(exec_state["EXECUTION STATE"].strip()) > 0 for exec_state in table):
                break

            _, table = self.__ank_cli.run("get workload", format_func=table_to_dict)
        return self

