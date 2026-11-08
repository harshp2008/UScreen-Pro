/*
 * This file is auto-generated.  DO NOT MODIFY.
 * Using: /home/harsh/Android/Sdk/build-tools/35.0.0/aidl -p/home/harsh/Android/Sdk/platforms/android-34/framework.aidl -o/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/build/generated/aidl_source_output_dir/debug/out -I/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/main/aidl -I/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/debug/aidl -d/tmp/aidl13406131529248624148.d /home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/main/aidl/com/samsung/android/sdk/penremote/ISpenEventListener.aidl
 */
package com.samsung.android.sdk.penremote;
/**
 * This service should be viewed as a member of ISpenRemoteService
 * 
 * Receives the events sent by the S Pen
 * 
 * There are two types of events:
 * * Button presses [Up/Down]
 * * AirMotionEvents [deltaX, deltaY]
 * 
 * An event listener will only have one event type provided.
 * The type of event is specified in the call to ISpenRemoteService
 */
public interface ISpenEventListener extends android.os.IInterface
{
  /** Default implementation for ISpenEventListener. */
  public static class Default implements com.samsung.android.sdk.penremote.ISpenEventListener
  {
    @Override public void onEvent(com.samsung.android.sdk.penremote.SpenEvent event) throws android.os.RemoteException
    {
    }
    @Override
    public android.os.IBinder asBinder() {
      return null;
    }
  }
  /** Local-side IPC implementation stub class. */
  public static abstract class Stub extends android.os.Binder implements com.samsung.android.sdk.penremote.ISpenEventListener
  {
    /** Construct the stub at attach it to the interface. */
    @SuppressWarnings("this-escape")
    public Stub()
    {
      this.attachInterface(this, DESCRIPTOR);
    }
    /**
     * Cast an IBinder object into an com.samsung.android.sdk.penremote.ISpenEventListener interface,
     * generating a proxy if needed.
     */
    public static com.samsung.android.sdk.penremote.ISpenEventListener asInterface(android.os.IBinder obj)
    {
      if ((obj==null)) {
        return null;
      }
      android.os.IInterface iin = obj.queryLocalInterface(DESCRIPTOR);
      if (((iin!=null)&&(iin instanceof com.samsung.android.sdk.penremote.ISpenEventListener))) {
        return ((com.samsung.android.sdk.penremote.ISpenEventListener)iin);
      }
      return new com.samsung.android.sdk.penremote.ISpenEventListener.Stub.Proxy(obj);
    }
    @Override public android.os.IBinder asBinder()
    {
      return this;
    }
    @Override public boolean onTransact(int code, android.os.Parcel data, android.os.Parcel reply, int flags) throws android.os.RemoteException
    {
      java.lang.String descriptor = DESCRIPTOR;
      if (code >= android.os.IBinder.FIRST_CALL_TRANSACTION && code <= android.os.IBinder.LAST_CALL_TRANSACTION) {
        data.enforceInterface(descriptor);
      }
      if (code == INTERFACE_TRANSACTION) {
        reply.writeString(descriptor);
        return true;
      }
      switch (code)
      {
        case TRANSACTION_onEvent:
        {
          com.samsung.android.sdk.penremote.SpenEvent _arg0;
          _arg0 = _Parcel.readTypedObject(data, com.samsung.android.sdk.penremote.SpenEvent.CREATOR);
          this.onEvent(_arg0);
          reply.writeNoException();
          break;
        }
        default:
        {
          return super.onTransact(code, data, reply, flags);
        }
      }
      return true;
    }
    private static class Proxy implements com.samsung.android.sdk.penremote.ISpenEventListener
    {
      private android.os.IBinder mRemote;
      Proxy(android.os.IBinder remote)
      {
        mRemote = remote;
      }
      @Override public android.os.IBinder asBinder()
      {
        return mRemote;
      }
      public java.lang.String getInterfaceDescriptor()
      {
        return DESCRIPTOR;
      }
      @Override public void onEvent(com.samsung.android.sdk.penremote.SpenEvent event) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _Parcel.writeTypedObject(_data, event, 0);
          boolean _status = mRemote.transact(Stub.TRANSACTION_onEvent, _data, _reply, 0);
          _reply.readException();
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
      }
    }
    static final int TRANSACTION_onEvent = (android.os.IBinder.FIRST_CALL_TRANSACTION + 0);
  }
  /** @hide */
  public static final java.lang.String DESCRIPTOR = "com.samsung.android.sdk.penremote.ISpenEventListener";
  public void onEvent(com.samsung.android.sdk.penremote.SpenEvent event) throws android.os.RemoteException;
  /** @hide */
  static class _Parcel {
    static private <T> T readTypedObject(
        android.os.Parcel parcel,
        android.os.Parcelable.Creator<T> c) {
      if (parcel.readInt() != 0) {
          return c.createFromParcel(parcel);
      } else {
          return null;
      }
    }
    static private <T extends android.os.Parcelable> void writeTypedObject(
        android.os.Parcel parcel, T value, int parcelableFlags) {
      if (value != null) {
        parcel.writeInt(1);
        value.writeToParcel(parcel, parcelableFlags);
      } else {
        parcel.writeInt(0);
      }
    }
  }
}
