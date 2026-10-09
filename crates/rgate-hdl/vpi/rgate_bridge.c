/* RGate's out-of-process VPI bridge. GPL-3.0-or-later. */
#include <vpi_user.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <inttypes.h>

#define MAX_BINDINGS 256
static vpiHandle bindings[MAX_BINDINGS];
static int probed[MAX_BINDINGS];
static int count;
static uint64_t ticks_per_ns;
static int pending_id;
static char pending_bits[4097];

static uint64_t now(void) {
    s_vpi_time time = {0}; time.type = vpiSimTime;
    vpi_get_time(NULL, &time);
    return ((uint64_t)time.high << 32) | time.low;
}
static void report_value(const char *kind, int id) {
    s_vpi_value value = {0}; value.format = vpiBinStrVal;
    vpi_get_value(bindings[id], &value);
    printf("RGATE %s %" PRIu64 " %d %s\n", kind, now(), id, value.value.str);
}
static PLI_INT32 pause_cb(p_cb_data data);
static void callback(int reason, uint64_t delay, PLI_INT32 (*fn)(p_cb_data)) {
    s_vpi_time time = {0}; time.type = vpiSimTime;
    time.high = (uint32_t)(delay >> 32); time.low = (uint32_t)delay;
    s_cb_data cb = {0}; cb.reason = reason; cb.cb_rtn = fn; cb.time = &time;
    vpi_register_cb(&cb);
}
static PLI_INT32 arrive_cb(p_cb_data data) {
    (void)data; callback(cbReadOnlySynch, 0, pause_cb); return 0;
}
static PLI_INT32 write_cb(p_cb_data data) {
    (void)data;
    s_vpi_value value = {0}; value.format = vpiBinStrVal; value.value.str = pending_bits;
    vpi_put_value(bindings[pending_id], &value, NULL, vpiNoDelay);
    callback(cbReadOnlySynch, 0, pause_cb); return 0;
}
static PLI_INT32 change_cb(p_cb_data data) {
    int id = (int)(intptr_t)data->user_data;
    report_value("CHANGE", id); fflush(stdout); return 0;
}
static void enumerate(vpiHandle scope) {
    int types[] = {vpiNet, vpiReg};
    for (unsigned i=0; i<sizeof(types)/sizeof(types[0]); ++i) {
        vpiHandle iter = vpi_iterate(types[i], scope), item;
        if (iter) while ((item=vpi_scan(iter))) {
            int width = vpi_get(vpiSize,item);
            printf("RGATE META %d %s\n",width,vpi_get_str(vpiFullName,item));
        }
    }
    vpiHandle iter=vpi_iterate(vpiModule,scope), item;
    if(iter) while((item=vpi_scan(iter))) enumerate(item);
}
static PLI_INT32 pause_cb(p_cb_data data) {
    (void)data;
snapshot:
    printf("RGATE TIME %" PRIu64 "\n", now());
    for(int i=0;i<count;++i) report_value("VALUE",i);
    puts("RGATE END"); fflush(stdout);
    char line[8192], path[4097];
    while(fgets(line,sizeof(line),stdin)) {
        uint64_t delay;
        int id;
        if(sscanf(line,"BIND %4096s",path)==1) {
            vpiHandle handle=vpi_handle_by_name(path,NULL);
            if(!handle || count>=MAX_BINDINGS) puts("RGATE ERROR missing-or-excess-binding");
            else {bindings[count++]=handle;}
            goto snapshot;
        }
        if(strncmp(line,"LIST",4)==0) {
            vpiHandle iter=vpi_iterate(vpiModule,NULL),item;
            if(iter) while((item=vpi_scan(iter))) enumerate(item);
            goto snapshot;
        }
        if(sscanf(line,"PROBE %d",&id)==1 && id>=0 && id<count) {
            s_vpi_time time={0};time.type=vpiSuppressTime;
            s_vpi_value value={0};value.format=vpiSuppressVal;
            s_cb_data cb={0};cb.reason=cbValueChange;cb.cb_rtn=change_cb;
            cb.obj=bindings[id];cb.time=&time;cb.value=&value;cb.user_data=(char *)(intptr_t)id;
            if(!probed[id]) {vpi_register_cb(&cb);probed[id]=1;}
            report_value("CHANGE",id);
            goto snapshot;
        }
        if(sscanf(line,"SET %d %4096s",&id,pending_bits)==2 && id>=0 && id<count) {
            /* A read-only pause cannot enqueue writes in the same time slot.
             * Deposit at the next precision tick, then settle before returning. */
            pending_id=id;callback(cbAfterDelay,1,write_cb);return 0;
        }
        if(sscanf(line,"RUN %" SCNu64,&delay)==1) {
            if(delay>UINT64_MAX/ticks_per_ns || delay*ticks_per_ns>UINT64_MAX-now()) {
                puts("RGATE ERROR time-overflow");goto snapshot;
            }
            if(delay==0) goto snapshot;
            callback(cbAfterDelay,delay*ticks_per_ns,arrive_cb);return 0;
        }
        if(strncmp(line,"STEP",4)==0) {callback(cbNextSimTime,0,arrive_cb);return 0;}
        if(strncmp(line,"QUIT",4)==0) {vpi_control(vpiFinish,0);return 0;}
        puts("RGATE ERROR invalid-command");fflush(stdout);
    }
    vpi_control(vpiFinish,0);return 0;
}
static PLI_INT32 start_cb(p_cb_data data) {
    (void)data;
    int precision=vpi_get(vpiTimePrecision,NULL);
    if(precision > -9 || precision < -15) {
        puts("RGATE ERROR unsupported-time-precision");fflush(stdout);vpi_control(vpiFinish,1);return 0;
    }
    ticks_per_ns=1;for(int i=precision;i < -9;++i) ticks_per_ns*=10;
    printf("RGATE SCALE %" PRIu64 "\n",ticks_per_ns);
    callback(cbReadOnlySynch,0,pause_cb);return 0;
}
static void register_bridge(void) {
    s_cb_data cb={0};cb.reason=cbStartOfSimulation;cb.cb_rtn=start_cb;vpi_register_cb(&cb);
}
void (*vlog_startup_routines[])(void) = {register_bridge, NULL};
