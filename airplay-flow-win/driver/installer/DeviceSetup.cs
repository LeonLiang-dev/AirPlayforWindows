using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

namespace AirPlayFlowWin.Installer
{
    public static class DeviceSetup
    {
        private const uint DICD_GENERATE_ID = 0x00000001;
        private const uint SPDRP_HARDWAREID = 0x00000001;
        private const uint DIF_REGISTERDEVICE = 0x00000019;
        private static readonly IntPtr InvalidHandleValue = new IntPtr(-1);

        [StructLayout(LayoutKind.Sequential)]
        private struct SP_DEVINFO_DATA
        {
            public uint cbSize;
            public Guid ClassGuid;
            public uint DevInst;
            public IntPtr Reserved;
        }

        [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool SetupDiGetINFClass(
            string infName,
            ref Guid classGuid,
            StringBuilder className,
            uint classNameSize,
            out uint requiredSize);

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern IntPtr SetupDiCreateDeviceInfoList(
            ref Guid classGuid,
            IntPtr hwndParent);

        [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool SetupDiCreateDeviceInfo(
            IntPtr deviceInfoSet,
            string deviceName,
            ref Guid classGuid,
            string deviceDescription,
            IntPtr hwndParent,
            uint creationFlags,
            ref SP_DEVINFO_DATA deviceInfoData);

        [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool SetupDiSetDeviceRegistryProperty(
            IntPtr deviceInfoSet,
            ref SP_DEVINFO_DATA deviceInfoData,
            uint property,
            byte[] propertyBuffer,
            uint propertyBufferSize);

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiCallClassInstaller(
            uint installFunction,
            IntPtr deviceInfoSet,
            ref SP_DEVINFO_DATA deviceInfoData);

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiDestroyDeviceInfoList(IntPtr deviceInfoSet);

        public static void CreateRootDevice(string infPath, string hardwareId)
        {
            if (String.IsNullOrWhiteSpace(infPath))
            {
                throw new ArgumentException("INF path is required.", "infPath");
            }
            if (String.IsNullOrWhiteSpace(hardwareId))
            {
                throw new ArgumentException("Hardware ID is required.", "hardwareId");
            }

            Guid classGuid = Guid.Empty;
            StringBuilder className = new StringBuilder(256);
            uint requiredSize;
            Ensure(
                SetupDiGetINFClass(
                    infPath,
                    ref classGuid,
                    className,
                    (uint)className.Capacity,
                    out requiredSize),
                "Unable to read the device setup class from the INF");

            IntPtr deviceInfoSet = SetupDiCreateDeviceInfoList(ref classGuid, IntPtr.Zero);
            if (deviceInfoSet == InvalidHandleValue)
            {
                throw LastError("Unable to create a device information set");
            }

            try
            {
                SP_DEVINFO_DATA deviceInfoData = new SP_DEVINFO_DATA();
                deviceInfoData.cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA));
                Ensure(
                    SetupDiCreateDeviceInfo(
                        deviceInfoSet,
                        className.ToString(),
                        ref classGuid,
                        null,
                        IntPtr.Zero,
                        DICD_GENERATE_ID,
                        ref deviceInfoData),
                    "Unable to create the root-enumerated device node");

                byte[] hardwareIds = Encoding.Unicode.GetBytes(hardwareId + "\0\0");
                Ensure(
                    SetupDiSetDeviceRegistryProperty(
                        deviceInfoSet,
                        ref deviceInfoData,
                        SPDRP_HARDWAREID,
                        hardwareIds,
                        (uint)hardwareIds.Length),
                    "Unable to assign the virtual audio hardware ID");

                Ensure(
                    SetupDiCallClassInstaller(
                        DIF_REGISTERDEVICE,
                        deviceInfoSet,
                        ref deviceInfoData),
                    "Unable to register the virtual audio device node");
            }
            finally
            {
                SetupDiDestroyDeviceInfoList(deviceInfoSet);
            }
        }

        private static void Ensure(bool result, string context)
        {
            if (!result)
            {
                throw LastError(context);
            }
        }

        private static Win32Exception LastError(string context)
        {
            int error = Marshal.GetLastWin32Error();
            return new Win32Exception(error, context + ": " + new Win32Exception(error).Message);
        }
    }
}
