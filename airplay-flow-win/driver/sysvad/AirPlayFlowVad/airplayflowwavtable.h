/*++

Copyright (c) AirPlay Flow Win contributors.

Module Name:

    airplayflowwavtable.h

Abstract:

    Minimal software-render endpoint for AirPlay Flow Win.

    Unlike the upstream SysVAD speaker endpoint, this filter deliberately does
    not expose a hardware audio engine, an offload pin, or a hardware loopback
    pin. WASAPI therefore provides software loopback by copying the Windows
    audio-engine mix. This avoids SysVAD's simulated 3 kHz loopback tone and
    lets Windows apply its software endpoint volume and mute controls.

--*/

#ifndef _AIRPLAYFLOW_WAVTABLE_H_
#define _AIRPLAYFLOW_WAVTABLE_H_

#define AIRPLAYFLOW_DEVICE_MAX_CHANNELS 2
#define AIRPLAYFLOW_MAX_SYSTEM_STREAMS  8

// Keep the device-side format identical to the RAOP transport format. The
// Windows audio engine can still accept application streams in other formats
// and mix/resample them before writing this pin.
static
KSDATAFORMAT_WAVEFORMATEXTENSIBLE AirPlayFlowHostPinSupportedDeviceFormats[] =
{
    {
        {
            sizeof(KSDATAFORMAT_WAVEFORMATEXTENSIBLE),
            0,
            0,
            0,
            STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM),
            STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
        },
        {
            {
                WAVE_FORMAT_EXTENSIBLE,
                2,
                44100,
                176400,
                4,
                16,
                sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX)
            },
            16,
            KSAUDIO_SPEAKER_STEREO,
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM)
        }
    }
};

static
MODE_AND_DEFAULT_FORMAT AirPlayFlowHostPinSupportedDeviceModes[] =
{
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_RAW,            &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat },
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_DEFAULT,        &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat },
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_MEDIA,          &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat },
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_MOVIE,          &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat },
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_COMMUNICATIONS, &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat },
    { STATIC_AUDIO_SIGNALPROCESSINGMODE_NOTIFICATION,   &AirPlayFlowHostPinSupportedDeviceFormats[0].DataFormat }
};

// The entries must follow the same order as AirPlayFlowWaveMiniportPins.
static
PIN_DEVICE_FORMATS_AND_MODES AirPlayFlowPinDeviceFormatsAndModes[] =
{
    {
        SystemRenderPin,
        AirPlayFlowHostPinSupportedDeviceFormats,
        SIZEOF_ARRAY(AirPlayFlowHostPinSupportedDeviceFormats),
        AirPlayFlowHostPinSupportedDeviceModes,
        SIZEOF_ARRAY(AirPlayFlowHostPinSupportedDeviceModes)
    },
    {
        BridgePin,
        NULL,
        0,
        NULL,
        0
    }
};

static
KSDATARANGE_AUDIO AirPlayFlowPinDataRangesStream[] =
{
    {
        {
            sizeof(KSDATARANGE_AUDIO),
            KSDATARANGE_ATTRIBUTES,
            0,
            0,
            STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM),
            STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
        },
        AIRPLAYFLOW_DEVICE_MAX_CHANNELS,
        16,
        16,
        44100,
        44100
    }
};

static
PKSDATARANGE AirPlayFlowPinDataRangePointersStream[] =
{
    PKSDATARANGE(&AirPlayFlowPinDataRangesStream[0]),
    PKSDATARANGE(&PinDataRangeAttributeList)
};

static
KSDATARANGE AirPlayFlowPinDataRangesBridge[] =
{
    {
        sizeof(KSDATARANGE),
        0,
        0,
        0,
        STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
        STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
        STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
    }
};

static
PKSDATARANGE AirPlayFlowPinDataRangePointersBridge[] =
{
    &AirPlayFlowPinDataRangesBridge[0]
};

static
PCPIN_DESCRIPTOR AirPlayFlowWaveMiniportPins[] =
{
    // Pin 0: the Windows software audio engine writes its mixed stream here.
    {
        AIRPLAYFLOW_MAX_SYSTEM_STREAMS,
        AIRPLAYFLOW_MAX_SYSTEM_STREAMS,
        0,
        NULL,
        {
            0,
            NULL,
            0,
            NULL,
            SIZEOF_ARRAY(AirPlayFlowPinDataRangePointersStream),
            AirPlayFlowPinDataRangePointersStream,
            KSPIN_DATAFLOW_IN,
            KSPIN_COMMUNICATION_SINK,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    },
    // Pin 1: bridge to the topology filter. No hardware loopback pin exists.
    {
        0,
        0,
        0,
        NULL,
        {
            0,
            NULL,
            0,
            NULL,
            SIZEOF_ARRAY(AirPlayFlowPinDataRangePointersBridge),
            AirPlayFlowPinDataRangePointersBridge,
            KSPIN_DATAFLOW_OUT,
            KSPIN_COMMUNICATION_NONE,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    }
};

static
PCCONNECTION_DESCRIPTOR AirPlayFlowWaveMiniportConnections[] =
{
    { PCFILTER_NODE, 0, PCFILTER_NODE, 1 }
};

static
PCPROPERTY_ITEM PropertiesAirPlayFlowWaveFilter[] =
{
    {
        &KSPROPSETID_Pin,
        KSPROPERTY_PIN_PROPOSEDATAFORMAT,
        KSPROPERTY_TYPE_SET | KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_WaveFilter
    },
    {
        &KSPROPSETID_Pin,
        KSPROPERTY_PIN_PROPOSEDATAFORMAT2,
        KSPROPERTY_TYPE_GET | KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_WaveFilter
    }
};

DEFINE_PCAUTOMATION_TABLE_PROP(AutomationAirPlayFlowWaveFilter, PropertiesAirPlayFlowWaveFilter);

static
PCFILTER_DESCRIPTOR AirPlayFlowWaveMiniportFilterDescriptor =
{
    0,
    &AutomationAirPlayFlowWaveFilter,
    sizeof(PCPIN_DESCRIPTOR),
    SIZEOF_ARRAY(AirPlayFlowWaveMiniportPins),
    AirPlayFlowWaveMiniportPins,
    sizeof(PCNODE_DESCRIPTOR),
    0,
    NULL,
    SIZEOF_ARRAY(AirPlayFlowWaveMiniportConnections),
    AirPlayFlowWaveMiniportConnections,
    0,
    NULL
};

#endif // _AIRPLAYFLOW_WAVTABLE_H_
