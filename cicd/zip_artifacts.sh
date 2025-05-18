echo "ls ./artifacts -lh"
ls ./artifacts -lh
cd ./artifacts

for file in *; do
    echo zip -r "${file}.zip" "$file"
    zip -r "${file}.zip" "$file"
    echo rm "$file"
    rm -r "$file"
done

cd ..
echo "ls ./artifacts -lh"
ls ./artifacts -lh