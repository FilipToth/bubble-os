import os
import dotenv

def run_qemu():
    if not os.path.exists('.env'):
        print('.env file not found')
        return

    dotenv.load_dotenv()
    env_vars = dotenv.dotenv_values('.env')

    if not env_vars.__contains__('OVMF_PATH'):
        print('OVMF_PATH not set in .env file')
        return

    ovmf_path = env_vars['OVMF_PATH']
    if not os.path.exists(ovmf_path):
        print('OVMF_PATH does not exist')
        return
    
    os.system('cargo build --profile=dev')
    
    print('Starting qemu on default gdp stub: localhost:1234 over HTTP')

    # add -s \ flag to enable gdb stub on default port
    # -d for log specifications, -D to add a log file, int for interrupts

    os.system(f"""
        qemu-system-x86_64 \
            -enable-kvm \
            -m 128 \
            -nographic \
            -bios {ovmf_path} \
            -device driver=e1000,netdev=n0 \
            -d int \
            -D qemu.log \
            -no-reboot \
            -no-shutdown \
            -netdev user,id=n0,tftp=target/x86_64-unknown-uefi/debug,bootfile=bubble-os.efi
    """)

run_qemu()