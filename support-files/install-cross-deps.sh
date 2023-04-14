triple=aarch64-linux-gnu
mycc=$triple-gcc
pfd=$($triple-gcc --print-sysroot)
deps=( "zlib-1.2.13.tar.xz" "elfutils-0.187.tar.bz2" )

mkdir -p tmp
cd tmp

tar -xf ../zlib-1.2.13.tar.xz
tar -xf ../elfutils-0.187.tar.bz2

cd zlib-1.2.13
CROSS_PREFIX=$triple- ./configure --prefix=$pfd
make -j12
sudo make install
cd ..

cd elfutils-0.187
./configure --host=$triple --prefix=$pfd --disable-debuginfod --disable-libdebuginfod
make -j12
sudo make install
cd ..
