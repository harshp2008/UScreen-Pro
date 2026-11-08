/*
 * This file is auto-generated.  DO NOT MODIFY.
 * Using: /home/harsh/Android/Sdk/build-tools/35.0.0/aidl -p/home/harsh/Android/Sdk/platforms/android-34/framework.aidl -o/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/build/generated/aidl_source_output_dir/debug/out -I/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/main/aidl -I/home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/debug/aidl -d/tmp/aidl2448977305512836673.d /home/harsh/Documents/GITHUB\ PROJECTS/UScreen/android/spen-remote/src/main/aidl/com/samsung/android/sdk/penremote/ISPenRemoteService.aidl
 */
package com.samsung.android.sdk.penremote;
/**
 * Responsible for subscribing to events of different types
 * 
 * @see SPenUnitType
 */
public interface ISPenRemoteService extends android.os.IInterface
{
  /** Default implementation for ISPenRemoteService. */
  public static class Default implements com.samsung.android.sdk.penremote.ISPenRemoteService
  {
    /**
     * Registers a listener for events of the provided type
     * 
     * @param type see SPenUnitType
     */
    @Override public void registerSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException
    {
    }
    /**
     * Unregisters a listener for events of the provided type
     * 
     * @param type see SPenUnitType
     */
    @Override public void unregisterSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException
    {
    }
    @Override
    public android.os.IBinder asBinder() {
      return null;
    }
  }
  /** Local-side IPC implementation stub class. */
  public static abstract class Stub extends android.os.Binder implements com.samsung.android.sdk.penremote.ISPenRemoteService
  {
    /** Construct the stub at attach it to the interface. */
    @SuppressWarnings("this-escape")
    public Stub()
    {
      this.attachInterface(this, DESCRIPTOR);
    }
    /**
     * Cast an IBinder object into an com.samsung.android.sdk.penremote.ISPenRemoteService interface,
     * generating a proxy if needed.
     */
    public static com.samsung.android.sdk.penremote.ISPenRemoteService asInterface(android.os.IBinder obj)
    {
      if ((obj==null)) {
        return null;
      }
      android.os.IInterface iin = obj.queryLocalInterface(DESCRIPTOR);
      if (((iin!=null)&&(iin instanceof com.samsung.android.sdk.penremote.ISPenRemoteService))) {
        return ((com.samsung.android.sdk.penremote.ISPenRemoteService)iin);
      }
      return new com.samsung.android.sdk.penremote.ISPenRemoteService.Stub.Proxy(obj);
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
        case TRANSACTION_registerSpenEventListener:
        {
          int _arg0;
          _arg0 = data.readInt();
          com.samsung.android.sdk.penremote.ISpenEventListener _arg1;
          _arg1 = com.samsung.android.sdk.penremote.ISpenEventListener.Stub.asInterface(data.readStrongBinder());
          this.registerSpenEventListener(_arg0, _arg1);
          reply.writeNoException();
          break;
        }
        case TRANSACTION_unregisterSpenEventListener:
        {
          int _arg0;
          _arg0 = data.readInt();
          com.samsung.android.sdk.penremote.ISpenEventListener _arg1;
          _arg1 = com.samsung.android.sdk.penremote.ISpenEventListener.Stub.asInterface(data.readStrongBinder());
          this.unregisterSpenEventListener(_arg0, _arg1);
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
    private static class Proxy implements com.samsung.android.sdk.penremote.ISPenRemoteService
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
      /**
       * Registers a listener for events of the provided type
       * 
       * @param type see SPenUnitType
       */
      @Override public void registerSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(type);
          _data.writeStrongInterface(listener);
          boolean _status = mRemote.transact(Stub.TRANSACTION_registerSpenEventListener, _data, _reply, 0);
          _reply.readException();
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
      }
      /**
       * Unregisters a listener for events of the provided type
       * 
       * @param type see SPenUnitType
       */
      @Override public void unregisterSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(type);
          _data.writeStrongInterface(listener);
          boolean _status = mRemote.transact(Stub.TRANSACTION_unregisterSpenEventListener, _data, _reply, 0);
          _reply.readException();
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
      }
    }
    static final int TRANSACTION_registerSpenEventListener = (android.os.IBinder.FIRST_CALL_TRANSACTION + 0);
    static final int TRANSACTION_unregisterSpenEventListener = (android.os.IBinder.FIRST_CALL_TRANSACTION + 1);
  }
  /** @hide */
  public static final java.lang.String DESCRIPTOR = "com.samsung.android.sdk.penremote.ISPenRemoteService";
  /**
   * Registers a listener for events of the provided type
   * 
   * @param type see SPenUnitType
   */
  public void registerSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException;
  /**
   * Unregisters a listener for events of the provided type
   * 
   * @param type see SPenUnitType
   */
  public void unregisterSpenEventListener(int type, com.samsung.android.sdk.penremote.ISpenEventListener listener) throws android.os.RemoteException;
}
